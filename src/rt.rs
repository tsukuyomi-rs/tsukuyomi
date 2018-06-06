use futures::prelude::*;
use hyper::body::Body;
use hyper::server::conn::Http;
use hyper::service::{NewService, Service};
use std::net::SocketAddr;
use std::sync::Arc;
use tokio;
use tokio::net::TcpListener;

pub type Result<T> = ::std::result::Result<T, ::failure::Error>;

pub fn serve<S>(new_service: S, addr: &SocketAddr) -> Result<()>
where
    S: NewService<ReqBody = Body, ResBody = Body> + Send + Sync + 'static,
    S::Future: Send,
    S::Service: Send,
    <S::Service as Service>::Future: Send,
{
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
