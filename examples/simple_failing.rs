// Will create an exporter with a single metric that does not change. Will then
// set the exporter as failing.

use std::net::SocketAddr;

use env_logger::{
    Builder,
    Env,
};
use log::info;
use prometheus_exporter::{
    prometheus::register_gauge,
    Exporter,
};

fn main() {
    // Setup logger with default level info so we can see the messages from
    // prometheus_exporter.
    Builder::from_env(Env::default().default_filter_or("info")).init();

    // Parse address used to bind exporter to.
    let addr_raw = "0.0.0.0:9184";
    let addr: SocketAddr = addr_raw.parse().expect("can not parse listen addr");

    // Create metric
    let metric = register_gauge!("simple_the_answer", "to everything")
        .expect("can not create gauge simple_the_answer");

    metric.set(42.0);

    // Start exporter with status_name simple_failing
    let exporter = Exporter::builder(addr)
        .with_status_name("simple_failing")
        .start()
        .expect("can not start exporter");

    // Get metrics from exporter
    let body = reqwest::blocking::get(format!("http://{addr_raw}/metrics"))
        .expect("can not get metrics from exporter")
        .text()
        .expect("can not body text from request");

    info!("Exporter metrics:\n{body}");

    exporter.set_status_failing();

    let response = reqwest::blocking::get(format!("http://{addr_raw}/metrics"))
        .expect("can not get metrics from exporter");

    let status = response.status();
    let body = response
        .text()
        .expect("can not extract body from failed response");

    info!("Failed status: {status}, Failed body:\n{body}");
}
