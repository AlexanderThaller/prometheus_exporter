// Will create an exporter with a single metric that does not change

use env_logger::{
    Builder,
    Env,
};
use prometheus::{
    __register_gauge,
    opts,
    register_gauge,
};
use prometheus_exporter::PrometheusExporter;
use std::net::SocketAddr;

fn main() {
    // Setup logger with default level info so we can see the messages from
    // prometheus_exporter.
    Builder::from_env(Env::default().default_filter_or("info")).init();

    // Parse address used to bind exporter to.
    let addr_raw = "0.0.0.0:9184";
    let addr: SocketAddr = addr_raw.parse().expect("can not parse listen addr");

    // Create metric
    let metric =
        register_gauge!("the_answer", "to everything").expect("can not create gauge the_answer");

    metric.set(42.0);

    // Start exporter and makes metrics available under http://0.0.0.0:9184/metrics
    PrometheusExporter::run(&addr).expect("can not run exporter");
}
