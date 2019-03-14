# prometheus_exporter

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

## Usage

Add this to your `Cargo.toml`:

```toml
[dependencies]
prometheus_exporter = "0.3"
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
