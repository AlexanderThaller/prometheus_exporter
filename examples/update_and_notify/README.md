# Example - update_and_notify

Small example that demonstrates how prometheus_exporter can be used to expose a
single metric which will be updated everytime a client calls the http interface
of the exporter.

Can be run by using `cargo run` and will listen on `http://0.0.0.0:9185/metrics`
by default.
