//! "Helper to export prometheus metrics using hyper."

#![deny(missing_docs)]

use std::net::SocketAddr;

use log::info;

use hyper::{
    rt::{
        self,
        Future,
    },
    service::service_fn_ok,
    Body,
    Method,
    Response,
    Server,
};
use prometheus::{
    Encoder,
    TextEncoder,
};

/// Struct that holds everything together.
pub struct PrometheusExporter {}

impl PrometheusExporter {
    /// Start the prometheus exporter and bind the hyper http server to the
    /// given socket.
    pub fn run(addr: &SocketAddr) {
        let service = move || {
            let encoder = TextEncoder::new();

            service_fn_ok(move |req| match (req.method(), req.uri().path()) {
                (&Method::GET, "/metrics") => {
                    let metric_families = prometheus::gather();
                    let mut buffer = vec![];
                    encoder.encode(&metric_families, &mut buffer).unwrap();

                    Response::new(Body::from(buffer))
                }
                _ => {
                    let message = "try /metrics for metrics\n";
                    Response::builder()
                        .status(301)
                        .body(Body::from(message))
                        .unwrap()
                }
            })
        };

        let server = Server::bind(&addr)
            .serve(service)
            .map_err(|e| eprintln!("server error: {}", e));

        info!("Listening on http://{}", addr);

        rt::run(server);
    }
}
