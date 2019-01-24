# prometheus_exporter

Helper libary to export prometheus metrics using hyper.

It uses https://github.com/pingcap/rust-prometheus[rust-prometheus] for
collecting and rendering the prometheus metrics and
https://github.com/hyperium/hyper[hyper] for exposing the metrics through http.

The usage is pretty simple. First you register all your metrics with the
provided macros:

```rust
let connection_state = register_int_gauge_vec!(
    vec![namespace, "tcp", "connection_state_total"].join("_"),
    "what state current tcp connections are in".to_string(),
    &["state"]
)
.expect("can not create tcp_connection_state_closed metric");
```

That is also where the metric values get updated:

```rust
connection_state
  .with_label_values(&["closed"])
  .set(1234);
```

After all the metrics are registerd and the updating is setup the exporter can
be started:
```rust
let addr = "0.0.0.0:19899".parse().expect("can not parse listen addr");
PrometheusExporter::run(addr);
```

This will block the thread it is executed in.

In the future the exporter will also provide a way to update the metrics when a
new request comes in but that still needs to be implemented.
