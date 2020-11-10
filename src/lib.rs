//! Helper to export prometheus metrics via http.

#![deny(missing_docs)]
#![warn(clippy::unwrap_used)]
#![warn(rust_2018_idioms, unused_lifetimes, missing_debug_implementations)]
#![forbid(unsafe_code)]

// Reexport prometheus so version missmatches don't happen.
pub use prometheus;

#[cfg(feature = "internal_metrics")]
use crate::prometheus::{
    register_counter,
    register_gauge,
    register_histogram,
    Counter,
    Gauge,
    Histogram,
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
use std::{
    net::SocketAddr,
    sync::mpsc::{
        sync_channel,
        Receiver,
        SyncSender,
    },
    thread,
};
use thiserror::Error;
use tiny_http::{
    Request,
    Response,
    Server as HTTPServer,
};

#[cfg(feature = "internal_metrics")]
lazy_static! {
    static ref HTTP_COUNTER: Counter = register_counter!(
        "prometheus_exporter_requests_total",
        "Number of HTTP requests made."
    )
    .expect("can not create HTTP_COUNTER metric. this should never fail");
    static ref HTTP_BODY_GAUGE: Gauge = register_gauge!(
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
    notify_sender: SyncSender<()>,
    request_receiver: Receiver<()>,
}

/// Helper to export prometheus metrics via http.
#[derive(Debug)]
struct Server {}

/// Create and start a new exporter which uses the given socket address to
/// export the metrics.
pub fn start(binding: SocketAddr) -> Result<Exporter, Error> {
    Builder::new(binding).start()
}

impl Builder {
    /// Create a new builder with the given binding.
    pub fn new(binding: SocketAddr) -> Builder {
        Self { binding }
    }

    /// Change binding of the builder to given binding.
    pub fn binding(self, binding: SocketAddr) -> Builder {
        Self { binding }
    }

    /// Create a new exporter based on the information from the builder.
    pub fn build(self) -> Exporter {
        todo!()
    }

    /// Create and start new exporter based on the information from the builder.
    pub fn start(self) -> Result<Exporter, Error> {
        let (request_sender, request_receiver) = sync_channel(0);
        let (notify_sender, notify_receiver) = sync_channel(0);

        let exporter = Exporter {
            request_receiver,
            notify_sender,
        };

        Server::start(self.binding, request_sender, notify_receiver)?;

        Ok(exporter)
    }
}

impl Exporter {
    /// Return new builder which will create a exporter once built.
    pub fn builder(binding: SocketAddr) -> Builder {
        Builder::new(binding)
    }

    /// Wait until a new request comes in.
    pub fn wait(&self) {
        self.request_receiver
            .recv()
            .expect("can not receive from request_receiver channel. this should never happen");
    }

    /// Notify exporter that metrics have been updated and should be sent
    /// through the http server.
    pub fn notify(&self) {
        self.notify_sender
            .send(())
            .expect("can not send to notify_sender channel. this should never happen");
    }
}

impl Server {
    fn start(
        binding: SocketAddr,
        request_sender: SyncSender<()>,
        notify_receiver: Receiver<()>,
    ) -> Result<(), Error> {
        let server = HTTPServer::http(&binding).map_err(Error::ServerStart)?;

        thread::spawn(move || {
            #[cfg(feature = "logging")]
            info!("listening on http://{}", binding);

            let encoder = TextEncoder::new();

            for request in server.incoming_requests() {
                #[cfg(feature = "internal_metrics")]
                HTTP_COUNTER.inc();

                #[cfg(feature = "internal_metrics")]
                let _timer = HTTP_REQ_HISTOGRAM.start_timer();

                request_sender
                    .send(())
                    .expect("can not send to request_sender. this should never happen");

                notify_receiver
                    .recv()
                    .expect("can not receive from notify_receiver. this should never happen");

                if let Err(err) = Self::process_request(request, &encoder) {
                    #[cfg(feature = "logging")]
                    error!("{}", err);

                    drop(err)
                }
            }
        });

        Ok(())
    }

    fn process_request(request: Request, encoder: &TextEncoder) -> Result<(), Error> {
        let metric_families = prometheus::gather();
        let mut buffer = vec![];

        encoder
            .encode(&metric_families, &mut buffer)
            .map_err(Error::EncodeMetrics)?;

        #[cfg(feature = "internal_metrics")]
        HTTP_BODY_GAUGE.set(buffer.len() as f64);

        let response = Response::from_data(buffer);
        request.respond(response).map_err(Error::Response)?;

        Ok(())
    }
}
