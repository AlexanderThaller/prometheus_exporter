# prometheus_exporter

[![Build Status](https://travis-ci.org/AlexanderThaller/prometheus_exporter.svg?branch=master)](https://travis-ci.org/AlexanderThaller/prometheus_exporter)
[![crates.io](https://img.shields.io/crates/v/prometheus_exporter.svg)](https://crates.io/crates/prometheus_exporter)
[![docs.rs](https://docs.rs/prometheus_exporter/badge.svg)](https://docs.rs/prometheus_exporter)

Helper libary to export prometheus metrics using hyper. It's intended to help
writing prometheus exporters without the need to setup and maintain a http
webserver. If the program also uses a http server for other purposes this
package is probably not the best way and
[rust-prometheus](https://github.com/pingcap/rust-prometheus) should be used
directly.

It uses [rust-prometheus](https://github.com/pingcap/rust-prometheus) for
collecting and rendering the prometheus metrics and
[hyper](https://github.com/hyperium/hyper) for exposing the metrics through
http.

**NOTICE:** You have to use the same prometheus crate version that is used by
this crate to make sure that the global registrar use by the prometheus macros
works as expected. Currently this crate uses prometheus version `0.9`.

## Usage

Add this to your `Cargo.toml`:

```toml
[dependencies]
prometheus_exporter = "0.5"
```

There are three ways on how to use the exporter:

* `PrometheusExporter::run`: Just starts the hyper http server and exports
    metrics from the global prometheus register under the path `/metrics`.
    Allows the most freedom on how and when to update/generate metrics.

* `PrometheusExporter::run_and_notify`: Starts the http server and opens
    channels to allow notification of when a request is made. This is usefull
    for cases where metrics should be updated everytime somebody calls the
    `/metrics` path.

* `PrometheusExporter::run_and_repeat`: Starts the http server and opens
    channels that will send a message to the original caller when a duration
    inteval is reached. This will be repeated forever. This is usefull for cases
    where metrics should be gathered in a set interval. For example if the
    metric collection is expensive it makes more sense to not collect the
    metrics all the time.

For examples on how to use `prometheus_exporter` see the examples folder.

A very simple example looks like this (from `examples/simple/src/main.rs`):

```rust
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
```
