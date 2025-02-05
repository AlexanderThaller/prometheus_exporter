//! TODO

use std::{
    convert::Infallible,
    net::SocketAddr,
    thread,
};

use hyper::{
    service::{
        make_service_fn,
        service_fn,
    },
    Body,
    Request,
    Response,
    Server,
};

use crate::Error;

/// TODO
pub fn start(binding: SocketAddr) -> Result<(), Error> {
    thread::spawn(move || {
        let runtime = tokio::runtime::Builder::new_current_thread()
            .thread_name("prometheus_exporter")
            .enable_io()
            .build()
            .unwrap();

        runtime.block_on(start_hyper(binding));
    });

    Ok(())
}

async fn start_hyper(binding: SocketAddr) {
    let make_service = make_service_fn(|_conn| async { Ok::<_, Infallible>(service_fn(handle)) });

    let server = Server::bind(&binding).serve(make_service);

    if let Err(e) = server.await {
        eprintln!("server error: {}", e);
    }
}

async fn handle(_req: Request<Body>) -> Result<Response<Body>, Infallible> {
    Ok(Response::new(Body::from("Hello World")))
}
