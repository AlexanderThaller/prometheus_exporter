//! Helper to export prometheus metrics via http.
//!
//! Information on how to use the prometheus crate can be found at
//! [`prometheus`].
//!
//! # Basic Example
//! The most basic usage of this crate is to just start a http server at the
//! given binding which will export all metrics registered on the global
//! prometheus registry:
//!
//! ```rust
//! use prometheus_exporter::{
//!     self,
//!     prometheus::register_counter,
//! };
//!
//! let binding = "127.0.0.1:9184".parse().unwrap();
//! // Will create an exporter and start the http server using the given binding.
//! // If the webserver can't bind to the given binding it will fail with an error.
//! prometheus_exporter::start(binding).unwrap();
//!
//! // Create a counter using the global prometheus registry and increment it by one.
//! // Notice that the macro is coming from the reexported prometheus crate instead
//! // of the original crate. This is important as different versions of the
//! // prometheus crate have incompatible global registries.
//! let counter = register_counter!("user_exporter_counter", "help").unwrap();
//! counter.inc();
//! ```
//!
//! # Wait for request
//! A probably more useful example in which the exporter waits until a request
//! comes in and then updates the metrics on the fly:
//! ```
//! use prometheus_exporter::{
//!     self,
//!     prometheus::register_counter,
//! };
//! # let barrier = std::sync::Arc::new(std::sync::Barrier::new(2));
//! # {
//! #   let barrier = barrier.clone();
//! #   std::thread::spawn(move || {
//! #     println!("client barrier");
//! #     barrier.wait();
//! #     let body = reqwest::blocking::get("http://127.0.0.1:9185").unwrap().text().unwrap();
//! #     println!("body = {:?}", body);
//! #   });
//! # }
//!
//! let binding = "127.0.0.1:9185".parse().unwrap();
//! let exporter = prometheus_exporter::start(binding).unwrap();
//! # barrier.wait();
//!
//! let counter = register_counter!("example_exporter_counter", "help").unwrap();
//!
//! // Wait will return a waitgroup so we need to bind the return value.
//! // The webserver will wait with responding to the request until the
//! // waitgroup has been dropped
//! let guard = exporter.wait_request();
//!
//! // Updates can safely happen after the wait. This has the advantage
//! // that metrics are always in sync with the exporter so you won't
//! // get half updated metrics.
//! counter.inc();
//!
//! // Drop the guard after metrics have been updated.
//! drop(guard);
//! ```
//!
//! # Update periodically
//! Another use case is to update the metrics periodically instead of updating
//! them for each request. This could be useful if the generation of the
//! metrics is expensive and shouldn't happen all the time.
//! ```
//! use prometheus_exporter::{
//!     self,
//!     prometheus::register_counter,
//! };
//! # let barrier = std::sync::Arc::new(std::sync::Barrier::new(2));
//! # {
//! #   let barrier = barrier.clone();
//! #   std::thread::spawn(move || {
//! #     println!("client barrier");
//! #     barrier.wait();
//! #     let body = reqwest::blocking::get("http://127.0.0.1:9186").unwrap().text().unwrap();
//! #     println!("body = {:?}", body);
//! #   });
//! # }
//!
//! let binding = "127.0.0.1:9186".parse().unwrap();
//! let exporter = prometheus_exporter::start(binding).unwrap();
//! # barrier.wait();
//!
//! let counter = register_counter!("example_exporter_counter", "help").unwrap();
//!
//! // Wait for one second and then update the metrics. `wait_duration` will
//! // return a mutex guard which makes sure that the http server won't
//! // respond while the metrics get updated.
//! let guard = exporter.wait_duration(std::time::Duration::from_millis(1000));
//! counter.inc();
//! drop(guard);
//! ```
//!
//! You can find examples under [`/examples`](https://github.com/AlexanderThaller/prometheus_exporter/tree/master/examples).
//!
//! # Crate Features
//! ## `logging`
//! *Enabled by default*: yes
//!
//! Enables startup logging and failed request logging using the
//! [`log`](https://crates.io/crates/log) crate.
//!
//! ## `internal_metrics`
//! *Enabled by default*: yes
//!
//! Enables the registration of internal metrics used by the crate. Will enable
//! the following metrics:
//! * `prometheus_exporter_requests_total`: Number of HTTP requests received.
//! * `prometheus_exporter_response_size_bytes`: The HTTP response sizes in
//!   bytes.
//! * `prometheus_exporter_request_duration_seconds`: The HTTP request latencies
//!   in seconds.

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
        Mutex,
        MutexGuard,
    },
    thread,
    time::Duration,
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
        "Number of HTTP requests received."
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
    update_lock: Arc<Mutex<()>>,
}

/// Helper to export prometheus metrics via http.
#[derive(Debug)]
struct Server {}

/// Create and start a new exporter which uses the given socket address to
/// export the metrics.
/// # Errors
///
/// Will return [`enum@Error`] if the http server fails to start for any reason.
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
    /// Will return [`enum@Error`] if the http server fails to start for any
    /// reason.
    pub fn start(self) -> Result<Exporter, Error> {
        let (request_sender, request_receiver) = sync_channel(0);
        let is_waiting = Arc::new(AtomicBool::new(false));
        let update_lock = Arc::new(Mutex::new(()));

        let exporter = Exporter {
            request_receiver,
            is_waiting: Arc::clone(&is_waiting),
            update_lock: Arc::clone(&update_lock),
        };

        Server::start(self.binding, request_sender, is_waiting, update_lock)?;

        Ok(exporter)
    }
}

impl Exporter {
    /// Return new builder which will create a exporter once built.
    #[must_use]
    pub fn builder(binding: SocketAddr) -> Builder {
        Builder::new(binding)
    }

    /// Wait until a new request comes in. Returns a mutex guard to make the
    /// http server wait until the metrics have been updated.
    #[must_use = "not using the guard will result in the exporter returning the prometheus data \
                  immediately over http"]
    pub fn wait_request(&self) -> MutexGuard<'_, ()> {
        self.is_waiting.store(true, Ordering::SeqCst);

        let update_waitgroup = self
            .request_receiver
            .recv()
            .expect("can not receive from request_receiver channel. this should never happen");

        self.is_waiting.store(false, Ordering::SeqCst);

        let guard = self
            .update_lock
            .lock()
            .expect("poisioned mutex. should never happen");

        update_waitgroup.wait();

        guard
    }

    /// Wait for given duration. Returns a mutex guard to make the http
    /// server wait until the metrics have been updated.
    #[must_use = "not using the guard will result in the exporter returning the prometheus data \
                  immediately over http"]
    pub fn wait_duration(&self, duration: Duration) -> MutexGuard<'_, ()> {
        thread::sleep(duration);

        self.update_lock
            .lock()
            .expect("poisioned mutex. should never happen")
    }
}

impl Server {
    fn start(
        binding: SocketAddr,
        request_sender: SyncSender<WaitGroup>,
        is_waiting: Arc<AtomicBool>,
        update_lock: Arc<Mutex<()>>,
    ) -> Result<(), Error> {
        let server = HTTPServer::http(&binding).map_err(Error::ServerStart)?;

        thread::spawn(move || {
            #[cfg(feature = "logging")]
            info!("exporting metrics to http://{}/metrics", binding);

            let encoder = TextEncoder::new();

            for request in server.incoming_requests() {
                if let Err(err) = match request.url() {
                    "/metrics" => Self::handler_metrics(
                        request,
                        &encoder,
                        &request_sender,
                        &is_waiting,
                        &update_lock,
                    ),

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
        update_lock: &Arc<Mutex<()>>,
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

        let _lock = update_lock
            .lock()
            .expect("poisioned mutex. should never happen");

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

use crossbeam_channel::{
    bounded,
    Receiver as OldReceiver,
    Sender,
};
use hyper::{
    rt::{
        self,
        Future,
    },
    service::service_fn_ok,
    Body,
    Error as HyperError,
    Method,
    Response as OldResponse,
    Server as OldServer,
};
use std::{
    error::Error as OldError,
    fmt,
};

/// Errors that can happen when the prometheus exporter gets started.
#[derive(Debug)]
pub enum StartError {
    /// Hyper related errors.
    HyperError(HyperError),
}

impl From<HyperError> for StartError {
    fn from(err: HyperError) -> Self {
        StartError::HyperError(err)
    }
}

impl fmt::Display for StartError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "{:?}", self)
    }
}

impl OldError for StartError {
    fn cause(&self) -> Option<&dyn OldError> {
        match self {
            StartError::HyperError(err) => Some(err),
        }
    }
}

/// Struct that holds everything together.
#[derive(Debug)]
pub struct PrometheusExporter;

/// Struct that will be sent when metrics should be updated.
#[derive(Debug)]
pub struct Update;

/// Struct that has to be sent when the update of the metrics was finished.
#[derive(Debug)]
pub struct FinishedUpdate;

impl PrometheusExporter {
    /// Start the prometheus exporter and bind the hyper http server to the
    /// given socket.
    /// # Errors
    /// Will return an error if hyper cant bind to the given socket.
    pub fn run(addr: &SocketAddr) -> Result<(), StartError> {
        let service = move || {
            let encoder = TextEncoder::new();

            service_fn_ok(move |req| match (req.method(), req.uri().path()) {
                (&Method::GET, "/metrics") => PrometheusExporter::send_metrics(&encoder),
                _ => PrometheusExporter::send_redirect(),
            })
        };

        let server = OldServer::try_bind(&addr)?
            .serve(service)
            .map_err(|e| log_serving_error(&e));

        log_startup(&addr);

        rt::run(server);

        Ok(())
    }

    /// Start the prometheus exporter, bind the hyper http server to the given
    /// socket and send messages when there are new requests for metrics.
    /// This is usefull if metrics should be updated everytime there is a
    /// requests.
    /// # Errors
    /// Will return an error if hyper cant bind to the given socket.
    #[must_use]
    pub fn run_and_notify(addr: SocketAddr) -> (OldReceiver<Update>, Sender<FinishedUpdate>) {
        let (update_sender, update_receiver) = bounded(0);
        let (finished_sender, finished_receiver) = bounded(0);

        thread::spawn(move || {
            let service = move || {
                let encoder = TextEncoder::new();
                let update_sender = update_sender.clone();
                let finished_receiver = finished_receiver.clone();

                service_fn_ok(move |req| match (req.method(), req.uri().path()) {
                    (&Method::GET, "/metrics") => {
                        update_sender
                            .send(Update {})
                            .expect("can not send update. this should never happen");
                        finished_receiver
                            .recv()
                            .expect("can not receive finish. this should never happen");

                        PrometheusExporter::send_metrics(&encoder)
                    }
                    _ => PrometheusExporter::send_redirect(),
                })
            };

            let server = OldServer::bind(&addr)
                .serve(service)
                .map_err(|e| log_serving_error(&e));

            log_startup(&addr);

            rt::run(server);
        });

        (update_receiver, finished_sender)
    }

    /// Starts the prometheus exporter with http and will continiously send a
    /// message to update the metrics and wait inbetween for the given
    /// duration.
    #[must_use]
    pub fn run_and_repeat(
        addr: SocketAddr,
        duration: std::time::Duration,
    ) -> (OldReceiver<Update>, Sender<FinishedUpdate>) {
        let (update_sender, update_receiver) = bounded(0);
        let (finished_sender, finished_receiver) = bounded(0);

        thread::spawn(move || {
            let service = move || {
                let encoder = TextEncoder::new();

                service_fn_ok(move |req| match (req.method(), req.uri().path()) {
                    (&Method::GET, "/metrics") => PrometheusExporter::send_metrics(&encoder),
                    _ => PrometheusExporter::send_redirect(),
                })
            };

            let server = OldServer::bind(&addr)
                .serve(service)
                .map_err(|e| log_serving_error(&e));

            log_startup(&addr);

            rt::run(server);
        });

        {
            thread::spawn(move || loop {
                thread::sleep(duration);

                update_sender
                    .send(Update {})
                    .expect("can not send update. this should never happen");
                finished_receiver
                    .recv()
                    .expect("can not receive finish. this should never happen");
            });
        }

        (update_receiver, finished_sender)
    }

    fn send_metrics(encoder: &TextEncoder) -> OldResponse<Body> {
        let metric_families = prometheus::gather();
        let mut buffer = vec![];
        encoder
            .encode(&metric_families, &mut buffer)
            .expect("can not encode metrics");

        OldResponse::new(Body::from(buffer))
    }

    fn send_redirect() -> OldResponse<Body> {
        let message = "try /metrics for metrics\n";
        OldResponse::builder()
            .status(301)
            .body(Body::from(message))
            .expect("can not build response")
    }
}

#[allow(unused)]
fn log_startup(addr: &SocketAddr) {
    #[cfg(feature = "log")]
    info!("Listening on http://{}", addr);
}

#[allow(unused)]
fn log_serving_error(error: &HyperError) {
    #[cfg(feature = "log")]
    error!("problem while serving metrics: {}", error)
}
