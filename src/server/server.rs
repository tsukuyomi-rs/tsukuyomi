use failure::Error;
use futures::prelude::*;
use hyper::body::Body;
use hyper::server::conn::Http;
use hyper::service::{NewService, Service};
use std::sync::Arc;
use tokio;

use super::conn::Connection;
use super::service::ServiceUpgradeExt;
use super::transport::{Incoming, Io};

// TODO: impl Future
// TODO: configure for transports

pub fn run<S>(new_service: S) -> Result<(), ::failure::Error>
where
    S: NewService<ReqBody = Body, ResBody = Body> + Send + Sync + 'static,
    S::Future: Send,
    S::Service: ServiceUpgradeExt<Io> + Send,
    <S::Service as Service>::Future: Send,
    <S::Service as ServiceUpgradeExt<Io>>::Upgrade: Send,
{
    let server = Server::new(new_service)?;
    ::tokio::run(server.serve());
    Ok(())
}

// ==== Server ====

#[derive(Debug)]
pub struct Server<S> {
    incoming: Incoming,
    new_service: Arc<S>,
    protocol: Arc<Http>,
}

impl<S> Server<S>
where
    S: NewService<ReqBody = Body, ResBody = Body> + Send + Sync + 'static,
    S::Future: Send,
    S::Service: ServiceUpgradeExt<Io> + Send,
    <S::Service as Service>::Future: Send,
    <S::Service as ServiceUpgradeExt<Io>>::Upgrade: Send,
{
    pub fn new(new_service: S) -> Result<Server<S>, Error> {
        let incoming = Incoming::builder().finish()?;
        Ok(Server {
            incoming,
            new_service: Arc::new(new_service),
            protocol: Arc::new(Http::new()),
        })
    }

    pub fn serve(self) -> impl Future<Item = (), Error = ()> + Send + 'static {
        let Server {
            new_service,
            protocol,
            incoming,
        } = self;

        incoming.map_err(|_| ()).for_each(move |handshake| {
            let protocol = protocol.clone();
            let new_service = new_service.clone();
            handshake.map_err(|_| ()).and_then(move |stream| {
                new_service.new_service().map_err(|_e| ()).and_then(move |service| {
                    let conn = Connection::Http(protocol.serve_connection(stream, service));
                    tokio::spawn(conn)
                })
            })
        })
    }
}
