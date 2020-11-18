//! Helper to export prometheus metrics via http.

#![deny(missing_docs)]
#![forbid(unsafe_code)]
#![warn(clippy::pedantic)]
#![warn(clippy::unwrap_used)]
#![warn(rust_2018_idioms, unused_lifetimes, missing_debug_implementations)]

// Reexport prometheus so version missmatches don't happen.
pub use prometheus;

#[cfg(feature = "internal_metrics")]
use crate::prometheus::{
    register_histogram,
    register_int_counter,
    register_int_gauge,
    Histogram,
    IntCounter,
    IntGauge,
};
#[cfg(feature = "internal_metrics")]
use lazy_static::lazy_static;
#[cfg(feature = "logging")]
use log::{
    error,
    info,
};

use crate::prometheus::{
    Encoder,
    TextEncoder,
};
use crossbeam_utils::sync::WaitGroup;
use std::{
    net::SocketAddr,
    sync::{
        atomic::{
            AtomicBool,
            Ordering,
        },
        mpsc::{
            sync_channel,
            Receiver,
            SyncSender,
        },
        Arc,
    },
    thread,
};
use thiserror::Error;
use tiny_http::{
    Header,
    Request,
    Response,
    Server as HTTPServer,
};

#[cfg(feature = "internal_metrics")]
lazy_static! {
    static ref HTTP_COUNTER: IntCounter = register_int_counter!(
        "prometheus_exporter_requests_total",
        "Number of HTTP requests made."
    )
    .expect("can not create HTTP_COUNTER metric. this should never fail");
    static ref HTTP_BODY_GAUGE: IntGauge = register_int_gauge!(
        "prometheus_exporter_response_size_bytes",
        "The HTTP response sizes in bytes."
    )
    .expect("can not create HTTP_BODY_GAUGE metric. this should never fail");
    static ref HTTP_REQ_HISTOGRAM: Histogram = register_histogram!(
        "prometheus_exporter_request_duration_seconds",
        "The HTTP request latencies in seconds."
    )
    .expect("can not create HTTP_REQ_HISTOGRAM metric. this should never fail");
}

/// Errors that can occur while building or running an exporter.
#[derive(Debug, Error)]
pub enum Error {
    /// Returned when trying to start the exporter and
    /// [`tiny_http::Server::http`] fails.
    #[error("can not start http server: {0}")]
    ServerStart(Box<dyn std::error::Error + Send + Sync + 'static>),
}

/// Errors that can occur while handling requests.
#[derive(Debug, Error)]
enum HandlerError {
    /// Returned when the encoding of the metrics by
    /// [`prometheus::Encoder::encode`] fails.
    #[error("can not encode metrics: {0}")]
    EncodeMetrics(prometheus::Error),

    /// Returned when returning encoded metrics via response with
    /// [`tiny_http::Request::respond`] fails.
    #[error("can not generate response: {0}")]
    Response(std::io::Error),
}

/// Builder to create a new [`crate::Exporter`].
#[derive(Debug)]
pub struct Builder {
    binding: SocketAddr,
}

/// Helper to export prometheus metrics via http.
#[derive(Debug)]
pub struct Exporter {
    request_receiver: Receiver<WaitGroup>,
    is_waiting: Arc<AtomicBool>,
}

/// Helper to export prometheus metrics via http.
#[derive(Debug)]
struct Server {}

/// Create and start a new exporter which uses the given socket address to
/// export the metrics.
/// # Errors
///
/// Will return `Err` if the http server fails to start for any reason.
pub fn start(binding: SocketAddr) -> Result<Exporter, Error> {
    Builder::new(binding).start()
}

impl Builder {
    /// Create a new builder with the given binding.
    #[must_use]
    pub fn new(binding: SocketAddr) -> Builder {
        Self { binding }
    }

    /// Create and start new exporter based on the information from the builder.
    /// # Errors
    ///
    /// Will return `Err` if the http server fails to start for any reason.
    pub fn start(self) -> Result<Exporter, Error> {
        let (request_sender, request_receiver) = sync_channel(0);
        let is_waiting = Arc::new(AtomicBool::new(false));

        let exporter = Exporter {
            request_receiver,
            is_waiting: Arc::clone(&is_waiting),
        };

        Server::start(self.binding, request_sender, is_waiting)?;

        Ok(exporter)
    }
}

impl Exporter {
    /// Return new builder which will create a exporter once built.
    #[must_use]
    pub fn builder(binding: SocketAddr) -> Builder {
        Builder::new(binding)
    }

    /// Wait until a new request comes in.
    #[must_use = "not using the waitgroup will result in the exporter returning the prometheus \
                  data immediately over http"]
    pub fn wait(&self) -> WaitGroup {
        self.is_waiting.store(true, Ordering::SeqCst);

        let update_barrier = self
            .request_receiver
            .recv()
            .expect("can not receive from request_receiver channel. this should never happen");

        self.is_waiting.store(false, Ordering::SeqCst);

        update_barrier
    }
}

impl Server {
    fn start(
        binding: SocketAddr,
        request_sender: SyncSender<WaitGroup>,
        is_waiting: Arc<AtomicBool>,
    ) -> Result<(), Error> {
        let server = HTTPServer::http(&binding).map_err(Error::ServerStart)?;

        thread::spawn(move || {
            #[cfg(feature = "logging")]
            info!("listening on http://{}", binding);

            let encoder = TextEncoder::new();

            for request in server.incoming_requests() {
                if let Err(err) = match request.url() {
                    "/metrics" => {
                        Self::handler_metrics(request, &encoder, &request_sender, &is_waiting)
                    }

                    _ => Self::handler_redirect(request),
                } {
                    #[cfg(feature = "logging")]
                    error!("{}", err);

                    // Just so there are no complains about unused err variable when logging
                    // feature is disabled
                    drop(err)
                }
            }
        });

        Ok(())
    }

    fn handler_metrics(
        request: Request,
        encoder: &TextEncoder,
        request_sender: &SyncSender<WaitGroup>,
        is_waiting: &Arc<AtomicBool>,
    ) -> Result<(), HandlerError> {
        #[cfg(feature = "internal_metrics")]
        HTTP_COUNTER.inc();

        #[cfg(feature = "internal_metrics")]
        let _timer = HTTP_REQ_HISTOGRAM.start_timer();

        if is_waiting.load(Ordering::SeqCst) {
            let wg = WaitGroup::new();

            request_sender
                .send(wg.clone())
                .expect("can not send to request_sender. this should never happen");

            wg.wait();
        }

        println!("process_request");
        Self::process_request(request, encoder)
    }

    fn process_request(request: Request, encoder: &TextEncoder) -> Result<(), HandlerError> {
        let metric_families = prometheus::gather();
        let mut buffer = vec![];

        encoder
            .encode(&metric_families, &mut buffer)
            .map_err(HandlerError::EncodeMetrics)?;

        #[cfg(feature = "internal_metrics")]
        HTTP_BODY_GAUGE.set(buffer.len() as i64);

        let response = Response::from_data(buffer);
        request.respond(response).map_err(HandlerError::Response)?;

        Ok(())
    }

    fn handler_redirect(request: Request) -> Result<(), HandlerError> {
        let response = Response::from_string("try /metrics for metrics\n".to_string())
            .with_status_code(301)
            .with_header(Header {
                field: "Location"
                    .parse()
                    .expect("can not parse location header field. this should never fail"),
                value: ascii::AsciiString::from_ascii("/metrics")
                    .expect("can not parse header value. this should never fail"),
            });

        request.respond(response).map_err(HandlerError::Response)?;

        Ok(())
    }
}
