use futures::prelude::*;
use hyper::server::conn::Http;
use hyper::service::NewService;
use std::net::SocketAddr;
use std::sync::Arc;
use tokio;
use tokio::net::TcpListener;

use service::NewMyService;

pub fn serve(new_service: NewMyService, addr: &SocketAddr) -> ::Result<()> {
    let protocol = Arc::new(Http::new());

    let server = TcpListener::bind(&addr)?
        .incoming()
        .map_err(|_e| ())
        .for_each(move |stream| {
            let protocol = protocol.clone();
            new_service
                .new_service()
                .map_err(|_e| ())
                .and_then(move |service| {
                    let conn = protocol.serve_connection(stream, service);
                    tokio::spawn(conn.then(|_| Ok(())))
                })
        });

    tokio::run(server);
    Ok(())
}
