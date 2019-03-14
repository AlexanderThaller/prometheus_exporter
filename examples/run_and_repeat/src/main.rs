// Will create an exporter with a single metric that will randomize the value
// of the metric everytime the exporter duration times out.

use env_logger::{
    Builder,
    Env,
};
use log::info;
use prometheus::{
    __register_gauge,
    opts,
    register_gauge,
};
use prometheus_exporter::{
    FinishedUpdate,
    PrometheusExporter,
};
use rand::Rng;
use std::net::SocketAddr;

fn main() {
    // Setup logger with default level info so we can see the messages from
    // prometheus_exporter.
    Builder::from_env(Env::default().default_filter_or("info")).init();

    // Parse address used to bind exporter to.
    let addr_raw = "0.0.0.0:9186";
    let addr: SocketAddr = addr_raw.parse().expect("can not parse listen addr");

    // Start exporter and update metrics every five seconds.
    let duration = std::time::Duration::from_secs(5);
    let (request_receiver, finished_sender) = PrometheusExporter::run_and_repeat(addr, duration);

    // Create metric
    let random_value_metric = register_gauge!("random_value_metric", "will set a random value")
        .expect("can not create gauge random_value_metric");

    let mut rng = rand::thread_rng();

    loop {
        // Will block until exporter duration times out.
        request_receiver.recv().unwrap();

        info!("Updating metrics");

        // Update metric with random value.
        random_value_metric.set(rng.gen());

        // Notify exporter that all metrics have been updated so the caller client can
        // receive a response.
        finished_sender.send(FinishedUpdate).unwrap();
    }
}
