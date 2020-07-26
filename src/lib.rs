//! "Helper to export prometheus metrics using hyper."

#![deny(missing_docs)]

use crossbeam_channel::{
    bounded,
    Receiver,
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
    Response,
    Server,
};
#[cfg(feature = "log")]
use log::{
    error,
    info,
};
pub use prometheus;
use prometheus::{
    Encoder,
    TextEncoder,
};
use std::{
    error::Error,
    fmt,
    net::SocketAddr,
    thread,
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
    fn fmt(&self, f: &mut fmt::Formatter) -> fmt::Result {
        write!(f, "{}", self)
    }
}

impl Error for StartError {
    fn cause(&self) -> Option<&dyn Error> {
        match self {
            StartError::HyperError(err) => Some(err),
        }
    }
}

/// Struct that holds everything together.
pub struct PrometheusExporter;

/// Struct that will be sent when metrics should be updated.
pub struct Update;

/// Struct that has to be sent when the update of the metrics was finished.
pub struct FinishedUpdate;

impl PrometheusExporter {
    /// Start the prometheus exporter and bind the hyper http server to the
    /// given socket.
    pub fn run(addr: &SocketAddr) -> Result<(), StartError> {
        let service = move || {
            let encoder = TextEncoder::new();

            service_fn_ok(move |req| match (req.method(), req.uri().path()) {
                (&Method::GET, "/metrics") => PrometheusExporter::send_metrics(&encoder),
                _ => PrometheusExporter::send_redirect(),
            })
        };

        let server = Server::try_bind(&addr)?
            .serve(service)
            .map_err(log_serving_error);

        log_startup(&addr);

        rt::run(server);

        Ok(())
    }

    /// Start the prometheus exporter, bind the hyper http server to the given
    /// socket and send messages when there are new requests for metrics.
    /// This is usefull if metrics should be updated everytime there is a
    /// requests.
    pub fn run_and_notify(addr: SocketAddr) -> (Receiver<Update>, Sender<FinishedUpdate>) {
        let (update_sender, update_receiver) = bounded(0);
        let (finished_sender, finished_receiver) = bounded(0);

        thread::spawn(move || {
            let service = move || {
                let encoder = TextEncoder::new();
                let update_sender = update_sender.clone();
                let finished_receiver = finished_receiver.clone();

                service_fn_ok(move |req| match (req.method(), req.uri().path()) {
                    (&Method::GET, "/metrics") => {
                        update_sender.send(Update {}).unwrap();
                        finished_receiver.recv().unwrap();

                        PrometheusExporter::send_metrics(&encoder)
                    }
                    _ => PrometheusExporter::send_redirect(),
                })
            };

            let server = Server::bind(&addr)
                .serve(service)
                .map_err(log_serving_error);

            log_startup(&addr);

            rt::run(server);
        });

        (update_receiver, finished_sender)
    }

    /// Starts the prometheus exporter with http and will continiously send a
    /// message to update the metrics and wait inbetween for the given
    /// duration.
    pub fn run_and_repeat(
        addr: SocketAddr,
        duration: std::time::Duration,
    ) -> (Receiver<Update>, Sender<FinishedUpdate>) {
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

            let server = Server::bind(&addr)
                .serve(service)
                .map_err(log_serving_error);

            log_startup(&addr);

            rt::run(server);
        });

        {
            thread::spawn(move || loop {
                thread::sleep(duration);

                update_sender.send(Update {}).unwrap();
                finished_receiver.recv().unwrap();
            });
        }

        (update_receiver, finished_sender)
    }

    fn send_metrics(encoder: &TextEncoder) -> Response<Body> {
        let metric_families = prometheus::gather();
        let mut buffer = vec![];
        encoder.encode(&metric_families, &mut buffer).unwrap();

        Response::new(Body::from(buffer))
    }

    fn send_redirect() -> Response<Body> {
        let message = "try /metrics for metrics\n";
        Response::builder()
            .status(301)
            .body(Body::from(message))
            .unwrap()
    }
}

#[allow(unused)]
fn log_startup(addr: &SocketAddr) {
    #[cfg(feature = "log")]
    info!("Listening on http://{}", addr);
}

#[allow(unused)]
fn log_serving_error(error: HyperError) {
    #[cfg(feature = "log")]
    error!("problem while serving metrics: {}", error)
}
