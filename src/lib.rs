//! "Helper to export prometheus metrics using hyper."

#![deny(missing_docs)]

use std::{
    net::SocketAddr,
    thread,
};

use log::{
    error,
    info,
};

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
use prometheus::{
    Encoder,
    TextEncoder,
};

/// Errors that can happen when the prometheus exporter gets started.
pub enum StartError {
    /// Hyper related errors.
    HyperError(HyperError),
}

impl From<HyperError> for StartError {
    fn from(err: HyperError) -> Self {
        StartError::HyperError(err)
    }
}

/// Struct that holds everything together.
pub struct PrometheusExporter;

/// Struct that will be sent when there is a new request in the run_and_nofity
/// function.
pub struct NewRequest;

/// Struct that has to be sent when the update was finished and the request
/// should send the metrics to the requester.
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
            .map_err(|e| error!("problem while serving metrics: {}", e));

        info!("Listening on http://{}", addr);

        rt::run(server);

        Ok(())
    }

    /// Start the prometheus exporter, bind the hyper http server to the given
    /// socket and send messages when there are new requests for metrics.
    /// This is usefull if metrics should be updated everytime there is a
    /// requests.
    pub fn run_and_notify(addr: SocketAddr) -> (Receiver<NewRequest>, Sender<FinishedUpdate>) {
        let (request_sender, request_receiver) = bounded(0);
        let (finished_sender, finished_receiver) = bounded(0);

        thread::spawn(move || {
            let service = move || {
                let encoder = TextEncoder::new();
                let request_sender = request_sender.clone();
                let finished_receiver = finished_receiver.clone();

                service_fn_ok(move |req| match (req.method(), req.uri().path()) {
                    (&Method::GET, "/metrics") => {
                        request_sender.send(NewRequest {}).unwrap();
                        finished_receiver.recv().unwrap();

                        PrometheusExporter::send_metrics(&encoder)
                    }
                    _ => PrometheusExporter::send_redirect(),
                })
            };

            let server = Server::bind(&addr)
                .serve(service)
                .map_err(|e| error!("problem while serving metrics: {}", e));

            info!("Listening on http://{}", addr);

            rt::run(server);
        });

        (request_receiver, finished_sender)
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
