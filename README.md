# prometheus_exporter

[![crates.io](https://img.shields.io/crates/v/prometheus_exporter.svg)](https://crates.io/crates/prometheus_exporter)
[![docs.rs](https://docs.rs/prometheus_exporter/badge.svg)](https://docs.rs/prometheus_exporter)

Helper libary to export prometheus metrics using hyper.

It uses [rust-prometheus](https://github.com/pingcap/rust-prometheus) for
collecting and rendering the prometheus metrics and
[hyper](https://github.com/hyperium/hyper) for exposing the metrics through
http.

## Usage

Add this to your `Cargo.toml`:

```toml
[dependencies]
prometheus_exporter = "0.2"
```

The usage is pretty simple. First you register all your metrics with the
prometheus macros that will add the metric to the global register:

```rust
let connection_state = register_int_gauge_vec!(
    vec![namespace, "tcp", "connection_state_total"].join("_"),
    "what state current tcp connections are in".to_string(),
    &["state"]
)
.expect("can not create tcp_connection_state_closed metric");
```

And update the metric value in the same context:

```rust
connection_state
  .with_label_values(&["closed"])
  .set(1234);
```

After all the metrics are registerd and the updating is setup the exporter can
be started:
```rust
use std::net::SocketAddr;

let addr: SocketAddr = "0.0.0.0:19899".parse().expect("can not parse listen addr");
PrometheusExporter::run(&addr);
```

This will block the thread it is executed in.

Alternatively you can use `run_and_notify` which will send a message over a
channel when a new request is received giving the possibility to update metrics
before they are sent out to a requester:

```rust
use prometheus_exporter::{
    FinishedUpdate,
    PrometheusExporter,
};

let addr: SocketAddr = "0.0.0.0:19899".parse().expect("can not parse listen addr");
let (request_receiver, finished_sender) = PrometheusExporter::run_and_notify(addr);

let metrics = Metrics::new("netstat_exporter");

loop {
    request_receiver.recv().unwrap();

    metrics.update();

    finished_sender.send(FinishedUpdate).unwrap();
}
```

More examples can be found in the `examples` folder.
