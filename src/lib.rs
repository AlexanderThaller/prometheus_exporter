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
//!
//! // Create an exporter and start the http server using the given binding.
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
//!
//! let counter = register_counter!("example_exporter_counter", "help").unwrap();
//!
//! # barrier.wait();
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
//!
//! let counter = register_counter!("example_exporter_counter", "help").unwrap();
//!
//! // Wait for one second and then update the metrics. `wait_duration` will
//! // return a mutex guard which makes sure that the http server won't
//! // respond while the metrics get updated.
//! let guard = exporter.wait_duration(std::time::Duration::from_millis(100));
//! # barrier.wait();
//! counter.inc();
//! drop(guard);
//! ```
//!
//! # Custom Wait
//! TODO
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
//! #     let body = reqwest::blocking::get("http://127.0.0.1:9187").unwrap().text().unwrap();
//! #     println!("body = {:?}", body);
//! #   });
//! # }
//!
//! let binding = "127.0.0.1:9187".parse().unwrap();
//! let exporter = prometheus_exporter::start(binding).unwrap();
//!
//! let counter = register_counter!("example_exporter_counter", "help").unwrap();
//!
//! // TODO
//! let guard = exporter.wait();
//! # barrier.wait();
//! counter.inc();
//! drop(guard);
//! ```
//!
//!
//! You can find examples under [`/examples`](https://github.com/AlexanderThaller/prometheus_exporter/tree/master/examples).
//!
//! # Indicating errors
//! When collecting metrics fails it is good to communicate that to
//! prometheus (see
//! <https://prometheus.io/docs/instrumenting/writing_exporters/#failed-scrapes>).
//! For that the [`Exporter`] struct has the functions
//! [`Exporter::set_status_failing()`] and
//! [`Exporter::set_status_failing_with_message`]. When set to
//! [`Status::Failing`] the Exporter will respond with a 500 status code and the
//! `up` metric set to `0`. When a error message is provided it will be printed
//! before the metrics when exporting.
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
//! Enables the registration of internal metrics used by the crate. Enables
//! the following metrics:
//! * `prometheus_exporter_requests_total`: Number of HTTP requests received.
//! * `prometheus_exporter_response_size_bytes`: The HTTP response sizes in
//!   bytes.
//! * `prometheus_exporter_request_duration_seconds`: The HTTP request latencies
//!   in seconds.
//!
//! This feature will not work in combination with using a custom registry.

#![deny(missing_docs)]
#![forbid(unsafe_code)]
#![warn(clippy::pedantic)]
#![warn(clippy::unwrap_used)]
#![warn(rust_2018_idioms, unused_lifetimes, missing_debug_implementations)]

#[cfg(test)]
mod test;

use either::Either;

// Reexport prometheus so version missmatches don't happen.
pub use prometheus;

#[cfg(feature = "internal_metrics")]
use prometheus::{
    register_histogram_with_registry,
    register_int_counter_with_registry,
};

use prometheus::register_int_gauge_with_registry;

#[cfg(feature = "internal_metrics")]
use crate::prometheus::{
    Histogram,
    IntCounter,
    IntGauge,
};
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
    io::Write,
    net::{
        SocketAddr,
        TcpListener,
    },
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
        Barrier,
        Mutex,
        MutexGuard,
        RwLock,
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
    StatusCode,
};

/// Errors that can occur while building or running an exporter.
#[derive(Debug, Error)]
pub enum Error {
    /// Returned when trying to start the exporter and
    /// [`tiny_http::Server::http`] fails.
    #[error("can not start http server: {0}")]
    ServerStart(Box<dyn std::error::Error + Send + Sync + 'static>),

    /// Returned when supplying a non-ascii endpoint to
    /// [`Builder::with_endpoint`].
    #[error("supplied endpoint is not valid ascii: {0}")]
    EndpointNotAscii(String),

    /// Returned when registering the up metric failed.
    #[error("failed to register up metric with registry: {0}")]
    RegisterUpMetric(prometheus::Error),
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
    binding: Either<SocketAddr, TcpListener>,
    endpoint: Endpoint,
    registry: prometheus::Registry,
    status_name: Option<String>,
}

#[derive(Debug)]
struct Endpoint(String);

impl Default for Endpoint {
    fn default() -> Self {
        Self("/metrics".to_string())
    }
}
#[cfg(not(feature = "internal_metrics"))]
struct InternalMetrics {}

#[cfg(feature = "internal_metrics")]
struct InternalMetrics {
    http_counter: IntCounter,
    http_body_gauge: IntGauge,
    http_req_histogram: Histogram,
}

/// Helper to export prometheus metrics via http.
#[derive(Debug)]
pub struct Exporter {
    request_receiver: Receiver<Arc<Barrier>>,
    is_waiting: Arc<AtomicBool>,
    status: Arc<RwLock<Status>>,
    update_lock: Arc<Mutex<()>>,
}

/// Helper to export prometheus metrics via http.
#[derive(Debug)]
struct Server {}

/// Represents the status of the exporter.
#[derive(Debug, Default)]
pub enum Status {
    /// Exporter encountered no error when collecting metrics.
    #[default]
    Ok,

    /// Exporter encountered an error when collecting metrics.
    Failing {
        /// Optional error message which will be returned when the exporter gets
        /// scraped.
        err: Option<String>,
    },
}

impl Status {
    /// Returns `true` when [`Status`] is [`Status::Ok`].
    #[must_use]
    pub fn ok(&self) -> bool {
        matches!(self, Status::Ok)
    }
}

/// Create and start a new exporter which uses the given socket address to
/// export the metrics.
/// # Errors
///
/// Returns [`enum@Error`] if the http server fails to start for any reason.
pub fn start(binding: SocketAddr) -> Result<Exporter, Error> {
    Builder::new(binding).start()
}

impl Builder {
    /// Create a new builder with the given binding.
    #[must_use]
    pub fn new(binding: SocketAddr) -> Builder {
        Self {
            binding: either::Left(binding),
            endpoint: Endpoint::default(),
            registry: prometheus::default_registry().clone(),
            status_name: None,
        }
    }

    /// Create a new builder with the given [`TcpListener`].
    #[must_use]
    pub fn new_listener(listener: TcpListener) -> Builder {
        Self {
            binding: either::Right(listener),
            endpoint: Endpoint::default(),
            registry: prometheus::default_registry().clone(),
            status_name: None,
        }
    }

    /// Sets the endpoint that the metrics will be served on. If the endpoint is
    /// not set with this method then the default `/metrics` will be used.
    /// # Errors
    ///
    /// Returns [`enum@Error`] if the supplied string slice is not valid
    /// ascii.
    pub fn with_endpoint(&mut self, endpoint: &str) -> Result<(), Error> {
        if !endpoint.is_ascii() {
            return Err(Error::EndpointNotAscii(endpoint.to_string()));
        }

        let mut clean_endpoint = String::from('/');
        clean_endpoint.push_str(endpoint.trim_matches('/'));
        self.endpoint = Endpoint(clean_endpoint);

        Ok(())
    }

    /// Sets the registry the metrics will be gathered from. If the registry is
    /// not set, the default registry provided by the prometheus crate will be
    /// used. If a custom registry is used, the metrics provided by the
    /// `internal_metrics` feature are not available.
    #[must_use]
    pub fn with_registry(mut self, registry: &prometheus::Registry) -> Self {
        self.registry = registry.clone();
        self
    }

    /// Set the binding to the given socket addr. The exporter will create a new
    /// [`TcpListener`] itself.
    ///
    /// Overrides existing binding set by [`Builder::with_listener`].
    #[must_use]
    pub fn with_binding(mut self, binding: SocketAddr) -> Self {
        self.binding = either::Left(binding);
        self
    }

    /// Set the name of the status metric. By default the metric is
    /// called `up`. When set the metric is called `{name}_up`.
    #[must_use]
    pub fn with_status_name(mut self, name: &str) -> Self {
        self.status_name = Some(name.to_owned());
        self
    }

    /// Set the binding to the given listener. The exporter will use
    /// the given [`TcpListener`] instead of creating a new one.
    ///
    /// Overrides existing binding set by [`Builder::with_binding`].
    #[must_use]
    pub fn with_listener(mut self, listener: TcpListener) -> Self {
        self.binding = either::Right(listener);
        self
    }

    /// Create and start new exporter based on the information from
    /// the builder.
    ///
    /// # Errors
    ///
    /// Returns [`enum@Error`] if the http server fails to start for any
    /// reason.
    pub fn start(self) -> Result<Exporter, Error> {
        let (request_sender, request_receiver) = sync_channel(0);
        let is_waiting = Arc::new(AtomicBool::default());
        let status = Arc::new(RwLock::new(Status::default()));
        let update_lock = Arc::new(Mutex::new(()));

        let status_metric_name = if let Some(name) = &self.status_name {
            format!("{name}_up")
        } else {
            "up".to_string()
        };

        let exporter = Exporter {
            request_receiver,
            is_waiting: Arc::clone(&is_waiting),
            status: Arc::clone(&status),
            update_lock: Arc::clone(&update_lock),
        };

        Server::start(
            self.binding,
            self.endpoint.0,
            request_sender,
            is_waiting,
            status,
            update_lock,
            self.registry,
            &status_metric_name,
        )?;

        Ok(exporter)
    }
}

impl Exporter {
    /// Return new builder which will create a exporter once built.
    ///
    /// Uses a given binding to create a new [`TcpListener`] when starting the
    /// http server.
    #[must_use]
    pub fn builder(binding: SocketAddr) -> Builder {
        Builder::new(binding)
    }

    #[must_use]
    /// Return new builder which will create a exporter once built.
    ///
    /// Uses a given [`TcpListener`] when starting the http server.
    pub fn builder_listener(listener: TcpListener) -> Builder {
        Builder::new_listener(listener)
    }

    /// TODO
    #[must_use = "not using the guard will result in the exporter returning the prometheus data \
                  immediately over http"]
    pub fn wait(&self) -> MutexGuard<'_, ()> {
        self.update_lock
            .lock()
            .expect("poisioned mutex. should never happen")
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

    /// Set `status` for the exporter. When set to [`Status::Failing`] the
    /// exporter will respond with a 500 (Internal Server Error) and the `up`
    /// metric set to `0`. When set to [`Status::Ok`] (which is the default) the
    /// exporter responds with a 200 (Ok) and the metrics from the
    /// registry that the exporter uses.
    pub fn set_status(&self, status: Status) {
        *self
            .status
            .write()
            .expect("posioned mutex hopefully never happens") = status;
    }

    /// Set `status` for the exporter to [`Status::Ok`].
    pub fn set_status_ok(&self) {
        self.set_status(Status::Ok);
    }

    /// Set `status` for the exporter to [`Status::Failing`]
    /// without an error message.
    pub fn set_status_failing(&self) {
        self.set_status(Status::Failing { err: None });
    }

    /// Set `status` for the exporter to [`Status::Failing`]
    /// with the given error message.
    pub fn set_status_failing_with_message(&self, err: Option<String>) {
        self.set_status(Status::Failing { err });
    }
}

impl Server {
    #[allow(clippy::too_many_arguments)]
    fn start(
        binding: Either<SocketAddr, TcpListener>,
        endpoint: String,
        request_sender: SyncSender<Arc<Barrier>>,
        is_waiting: Arc<AtomicBool>,
        status: Arc<RwLock<Status>>,
        update_lock: Arc<Mutex<()>>,
        registry: prometheus::Registry,
        status_metric_name: &str,
    ) -> Result<(), Error> {
        let server = match binding {
            either::Left(binding) => {
                #[cfg(feature = "logging")]
                info!("exporting metrics to http://{binding}{endpoint}");

                HTTPServer::http(binding).map_err(Error::ServerStart)?
            }

            either::Right(listener) => {
                #[cfg(feature = "logging")]
                info!(
                    "exporting metrics to http://{}",
                    listener
                        .local_addr()
                        .expect("can not get listener local_addr. this should never happen")
                );

                HTTPServer::from_listener(listener, None).map_err(Error::ServerStart)?
            }
        };

        #[cfg(not(feature = "internal_metrics"))]
        let internal_metrics = InternalMetrics {};

        #[cfg(feature = "internal_metrics")]
        let internal_metrics = {
            let http_counter = register_int_counter_with_registry!(
                "prometheus_exporter_requests_total",
                "Number of HTTP requests received.",
                registry
            )
            .expect("can not create http_counter metric. this should never fail");

            let http_body_gauge = register_int_gauge_with_registry!(
                "prometheus_exporter_response_size_bytes",
                "The HTTP response sizes in bytes.",
                registry
            )
            .expect("can not create http_body_gauge metric. this should never fail");

            let http_req_histogram = register_histogram_with_registry!(
                "prometheus_exporter_request_duration_seconds",
                "The HTTP request latencies in seconds.",
                registry
            )
            .expect("can not create http_req_histogram metric. this should never fail");

            InternalMetrics {
                http_counter,
                http_body_gauge,
                http_req_histogram,
            }
        };

        let failed_registry = prometheus::Registry::new();

        register_int_gauge_with_registry!(status_metric_name, "status of the collector", registry)
            .map_err(Error::RegisterUpMetric)?
            .set(1);

        register_int_gauge_with_registry!(
            status_metric_name,
            "status of the collector",
            failed_registry
        )
        .expect(
            "failed to register status metric. should never fail as the failed_registry is only \
             used internally",
        )
        .set(0);

        thread::spawn(move || {
            let encoder = TextEncoder::new();

            for request in server.incoming_requests() {
                if let Err(err) = if request.url() == endpoint {
                    Self::handler_metrics(
                        request,
                        &encoder,
                        &request_sender,
                        &is_waiting,
                        &status,
                        &update_lock,
                        &registry,
                        &failed_registry,
                        &internal_metrics,
                    )
                } else {
                    Self::handler_redirect(request, &endpoint)
                } {
                    #[cfg(feature = "logging")]
                    error!("{}", err);

                    // Just so there are no complains about unused err variable when logging
                    // feature is disabled
                    drop(err);
                }
            }
        });

        Ok(())
    }

    #[allow(clippy::too_many_arguments)]
    fn handler_metrics(
        request: Request,
        encoder: &TextEncoder,
        request_sender: &SyncSender<Arc<Barrier>>,
        is_waiting: &Arc<AtomicBool>,
        status: &Arc<RwLock<Status>>,
        update_lock: &Arc<Mutex<()>>,
        registry: &prometheus::Registry,
        failed_registry: &prometheus::Registry,
        internal_metrics: &InternalMetrics,
    ) -> Result<(), HandlerError> {
        #[cfg(feature = "internal_metrics")]
        internal_metrics.http_counter.inc();

        #[cfg(feature = "internal_metrics")]
        let timer = internal_metrics.http_req_histogram.start_timer();

        if is_waiting.load(Ordering::SeqCst) {
            let barrier = Arc::new(Barrier::new(2));

            request_sender
                .send(Arc::clone(&barrier))
                .expect("can not send to request_sender. this should never happen");

            barrier.wait();
        }

        let _lock = update_lock
            .lock()
            .expect("poisioned mutex. should never happen");

        #[cfg(feature = "internal_metrics")]
        drop(timer);

        match &*status
            .read()
            .expect("lets hope there is no poisioned mutex")
        {
            Status::Ok => Self::process_request(
                request,
                encoder,
                registry,
                StatusCode(200),
                &None,
                internal_metrics,
            ),

            Status::Failing { err } => Self::process_request(
                request,
                encoder,
                failed_registry,
                StatusCode(500),
                err,
                internal_metrics,
            ),
        }
    }

    fn process_request(
        request: Request,
        encoder: &TextEncoder,
        registry: &prometheus::Registry,
        status_code: StatusCode,
        message: &Option<String>,

        #[allow(unused_variables)] internal_metrics: &InternalMetrics,
    ) -> Result<(), HandlerError> {
        let metric_families = registry.gather();
        let mut buffer = vec![];

        if let Some(message) = message {
            buffer
                .write_all(message.as_bytes())
                .expect("should never fail");

            buffer
                .write_all("\n\n".as_bytes())
                .expect("should never fail");
        }

        encoder
            .encode(&metric_families, &mut buffer)
            .map_err(HandlerError::EncodeMetrics)?;

        #[cfg(feature = "internal_metrics")]
        internal_metrics.http_body_gauge.set(buffer.len() as i64);

        let response = Response::from_data(buffer).with_status_code(status_code);
        request.respond(response).map_err(HandlerError::Response)?;

        Ok(())
    }

    fn handler_redirect(request: Request, endpoint: &str) -> Result<(), HandlerError> {
        let response = Response::from_string(format!("try {endpoint} for metrics\n"))
            .with_status_code(301)
            .with_header(Header {
                field: "Location"
                    .parse()
                    .expect("can not parse location header field. this should never fail"),
                value: ascii::AsciiString::from_ascii(endpoint)
                    .expect("can not parse header value. this should never fail"),
            });

        request.respond(response).map_err(HandlerError::Response)?;

        Ok(())
    }
}
