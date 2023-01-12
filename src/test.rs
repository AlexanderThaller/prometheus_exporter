use std::net::TcpListener;

use crate::prometheus::{
    register_counter_with_registry,
    Registry,
};

fn port_is_available(port: u16) -> Option<(u16, TcpListener)> {
    if let Ok(listener) = TcpListener::bind(("127.0.0.1", port)) {
        Some((port, listener))
    } else {
        None
    }
}

fn get_available_port() -> Option<(u16, TcpListener)> {
    (6000..10000).find_map(port_is_available)
}

fn get_binding() -> (String, TcpListener) {
    let (port, listener) = get_available_port().expect("unable to get a free port");

    (format!("127.0.0.1:{port}"), listener)
}

#[test]
fn wait_request() {
    let (binding_raw, listener) = get_binding();
    let metric_name = "test_wait_request";

    let barrier = std::sync::Arc::new(std::sync::Barrier::new(2));

    {
        let barrier = barrier.clone();
        let binding_raw = binding_raw.clone();

        std::thread::spawn(move || {
            println!("client barrier");
            barrier.wait();

            reqwest::blocking::get(format!("http://{binding_raw}")).expect("can not make request");
        });
    }

    let registry = Registry::new();

    let exporter = crate::Exporter::builder_listener(listener)
        .with_registry(&registry)
        .start()
        .expect("can not start exporter");

    let counter = register_counter_with_registry!(metric_name, "help", registry).unwrap();

    barrier.wait();

    let guard = exporter.wait_request();
    counter.inc();
    drop(guard);

    let body = reqwest::blocking::get(format!("http://{binding_raw}"))
        .expect("can not make request")
        .text()
        .expect("can not extract body");

    println!("body:\n{body}");

    assert!(body.contains(&format!("{metric_name} 1")));
}

#[test]
fn wait_duration() {
    let (binding_raw, listener) = get_binding();
    let metric_name = "test_wait_duration_counter";

    let registry = Registry::new();

    let exporter = crate::Exporter::builder_listener(listener)
        .with_registry(&registry)
        .start()
        .expect("can not start exporter");

    let counter = register_counter_with_registry!(metric_name, "help", registry).unwrap();

    let guard = exporter.wait_duration(std::time::Duration::from_millis(100));
    counter.inc();
    drop(guard);

    let body = reqwest::blocking::get(format!("http://{binding_raw}"))
        .expect("can not make request")
        .text()
        .expect("can not extract body");

    println!("body:\n{body}");

    assert!(body.contains(&format!("{metric_name} 1")));
}

#[test]
fn set_failed() {
    const ERROR_MESSAGE: &str = "testing the error message";

    let (binding_raw, listener) = get_binding();
    let metric_name = "test_wait_request";

    let barrier = std::sync::Arc::new(std::sync::Barrier::new(2));

    {
        let barrier = barrier.clone();
        let binding_raw = binding_raw.clone();

        std::thread::spawn(move || {
            println!("client barrier");
            barrier.wait();

            reqwest::blocking::get(format!("http://{binding_raw}")).expect("can not make request");
        });
    }

    let registry = Registry::new();

    let exporter = crate::Exporter::builder_listener(listener)
        .with_registry(&registry)
        .start()
        .expect("can not start exporter");

    let counter = register_counter_with_registry!(metric_name, "help", registry).unwrap();

    barrier.wait();

    let guard = exporter.wait_request();
    counter.inc();
    drop(guard);

    let response =
        reqwest::blocking::get(format!("http://{binding_raw}")).expect("can not make request");

    let status = response.status();
    let body = response.text().expect("can not extract body");

    assert_eq!(status, 200);

    println!("body:\n{body}");

    assert!(body.contains(&format!("{metric_name} 1")));
    assert!(body.contains("up 1"));

    exporter.set_status_failing_with_message(Some(ERROR_MESSAGE.to_string()));

    let response =
        reqwest::blocking::get(format!("http://{binding_raw}")).expect("can not make request");

    let status = response.status();
    let body = response.text().expect("can not extract body");

    assert_eq!(status, 500);

    println!("body:\n{body}");

    assert!(body.contains(ERROR_MESSAGE));
    assert!(!body.contains(&format!("{metric_name} 1")));
    assert!(body.contains("up 0"));

    exporter.set_status_ok();

    let response =
        reqwest::blocking::get(format!("http://{binding_raw}")).expect("can not make request");

    let status = response.status();
    let body = response.text().expect("can not extract body");

    assert_eq!(status, 200);

    println!("body:\n{body}");

    assert!(body.contains(&format!("{metric_name} 1")));
    assert!(body.contains("up 1"));
}
