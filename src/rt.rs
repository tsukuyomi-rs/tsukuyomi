use bytes::Bytes;
use failure::Error;
use futures::prelude::*;
use futures::Poll;
use hyper::body::Body;
use hyper::server::conn::{self, Http, Parts};
use hyper::service::{NewService, Service};
use std::sync::Arc;
use std::{fmt, mem};
use tokio;
use tokio::io::{AsyncRead, AsyncWrite};

pub trait ServiceExt<T>: Service + Sized {
    type Upgrade: Future<Item = (), Error = ()>;
    type UpgradeError: Into<Error>;

    fn poll_ready_upgrade(&mut self) -> Poll<(), Self::UpgradeError>;

    fn upgrade(self, io: T, read_buf: Bytes) -> Self::Upgrade;
}

pub fn serve<S, I>(new_service: S, incoming: I) -> Result<(), Error>
where
    S: NewService<ReqBody = Body, ResBody = Body> + Send + Sync + 'static,
    S::Future: Send,
    S::Service: ServiceExt<<I::Item as Future>::Item> + Send,
    <S::Service as Service>::Future: Send,
    <S::Service as ServiceExt<<I::Item as Future>::Item>>::Upgrade: Send,
    I: Stream + Send + 'static,
    I::Item: Future + Send + 'static,
    <I::Item as Future>::Item: AsyncRead + AsyncWrite + Send + 'static,
{
    let protocol = Arc::new(Http::new());
    let new_service = Arc::new(new_service);

    let server = incoming.map_err(|_| ()).for_each(move |handshake| {
        let protocol = protocol.clone();
        let new_service = new_service.clone();
        handshake.map_err(|_| ()).and_then(move |stream| {
            new_service
                .new_service()
                .map_err(|_e| ())
                .and_then(move |service| {
                    tokio::spawn(Connection::Http(protocol.serve_connection(stream, service)))
                })
        })
    });

    tokio::run(server);

    Ok(())
}

enum Connection<I, S>
where
    S: Service<ReqBody = Body, ResBody = Body> + ServiceExt<I>,
{
    Http(conn::Connection<I, S>),
    Upgrading(Parts<I, S>),
    Upgrade(S::Upgrade),
    Done,
}

impl<I, S> fmt::Debug for Connection<I, S>
where
    S: Service<ReqBody = Body, ResBody = Body> + ServiceExt<I> + fmt::Debug,
    I: fmt::Debug,
    S::Upgrade: fmt::Debug,
{
    fn fmt(&self, f: &mut fmt::Formatter) -> fmt::Result {
        match *self {
            Connection::Http(ref conn) => f.debug_tuple("Http").field(conn).finish(),
            Connection::Upgrading(ref parts) => f.debug_tuple("Upgrading").field(parts).finish(),
            Connection::Upgrade(ref fut) => f.debug_tuple("Upgrade").field(fut).finish(),
            Connection::Done => f.debug_tuple("Done").finish(),
        }
    }
}

impl<I, S> Future for Connection<I, S>
where
    I: AsyncRead + AsyncWrite + 'static,
    S: Service<ReqBody = Body, ResBody = Body> + ServiceExt<I> + 'static,
    S::Future: Send,
{
    type Item = ();
    type Error = ();

    fn poll(&mut self) -> Poll<Self::Item, Self::Error> {
        match self.poll_inner() {
            Ok(x) => Ok(x),
            Err(e) => {
                // TODO: reporting errors
                trace!("error during serving a connection: {}", e);
                *self = Connection::Done;
                Err(())
            }
        }
    }
}

impl<I, S> Connection<I, S>
where
    I: AsyncRead + AsyncWrite + 'static,
    S: Service<ReqBody = Body, ResBody = Body> + ServiceExt<I> + 'static,
    S::Future: Send,
{
    fn poll_inner(&mut self) -> Poll<(), Error> {
        use self::Connection::*;
        loop {
            match *self {
                Http(ref mut conn) => try_ready!(conn.poll_without_shutdown()),
                Upgrading(ref mut parts) => try_ready!(
                    parts
                        .service
                        .poll_ready_upgrade()
                        .map_err(Into::<Error>::into)
                ),
                Upgrade(ref mut fut) => {
                    try_ready!(fut.poll().map_err(|_| format_err!("during upgrade")))
                }
                Done => panic!("Connection has already been resolved or rejected"),
            }

            // assert: all of Futures has been already resolved or rejected.
            // It means that there is no incomplete and immovable Future at this point.

            // transit to the next state
            match mem::replace(self, Done) {
                Http(conn) => match conn.try_into_parts() {
                    Some(parts) => {
                        trace!("transit to Upgrading");
                        *self = Upgrading(parts);
                    }
                    None => {
                        trace!("the connection is h2");
                        return Ok(().into());
                    }
                },
                Upgrading(parts) => {
                    trace!("construct a future and transit to Upgrade");
                    let Parts {
                        service,
                        io,
                        read_buf,
                        ..
                    } = parts;
                    *self = Upgrade(service.upgrade(io, read_buf));
                }
                Upgrade(..) => return Ok(().into()),
                Done => unreachable!(),
            }
        }
    }
}
