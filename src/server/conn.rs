use futures::{Async, Future, Poll};
use hyper::body::{Body, Payload};
use hyper::server::conn::{self, Parts};
use hyper::service::Service;
use std::{fmt, mem};
use tokio::io::{AsyncRead, AsyncWrite};

use error::CritError;

use super::ServiceUpgradeExt;

pub enum Connection<I, S>
where
    S: Service<ReqBody = Body> + ServiceUpgradeExt<I>,
    S::ResBody: Payload,
    I: AsyncRead + AsyncWrite,
{
    Http(conn::Connection<I, S>),
    Upgrading(Parts<I, S>),
    Upgrade(S::Upgrade),
    Done,
}

impl<I, S> fmt::Debug for Connection<I, S>
where
    S: Service<ReqBody = Body> + ServiceUpgradeExt<I> + fmt::Debug,
    S::ResBody: Payload,
    I: AsyncRead + AsyncWrite + fmt::Debug,
    S::Upgrade: fmt::Debug,
{
    fn fmt(&self, f: &mut fmt::Formatter) -> fmt::Result {
        use self::Connection::*;
        match *self {
            Http(ref conn) => f.debug_tuple("Http").field(conn).finish(),
            Upgrading(ref parts) => f.debug_tuple("Upgrading").field(parts).finish(),
            Upgrade(ref fut) => f.debug_tuple("Upgrade").field(fut).finish(),
            Done => f.debug_tuple("Done").finish(),
        }
    }
}

impl<I, S> Future for Connection<I, S>
where
    I: AsyncRead + AsyncWrite + 'static,
    S: Service<ReqBody = Body> + ServiceUpgradeExt<I> + 'static,
    S::ResBody: Payload,
    S::Future: Send,
{
    type Item = ();
    type Error = ();

    fn poll(&mut self) -> Poll<Self::Item, Self::Error> {
        loop {
            match self.poll_ready() {
                Ok(Async::NotReady) => return Ok(Async::NotReady),
                Ok(Async::Ready(())) => {}
                Err(e) => {
                    trace!("error during serving a connection: {}", e);

                    // TODO: reporting errors
                    *self = Connection::Done;

                    //
                    return Ok(Async::Ready(()));
                }
            }

            // assertion: all Futures has been already resolved or rejected at this point.
            // It means that there is no incomplete and immovable Futures.

            let terminated = self.next_state();
            if terminated {
                break Ok(Async::Ready(()));
            }
        }
    }
}

impl<I, S> Connection<I, S>
where
    I: AsyncRead + AsyncWrite + 'static,
    S: Service<ReqBody = Body> + ServiceUpgradeExt<I> + 'static,
    S::ResBody: Payload,
    S::Future: Send,
{
    fn poll_ready(&mut self) -> Poll<(), CritError> {
        use self::Connection::*;
        match *self {
            Http(ref mut conn) => conn.poll_without_shutdown().map_err(Into::into),
            Upgrading(ref mut parts) => parts.service.poll_ready_upgradable().map_err(Into::into),
            Upgrade(ref mut fut) => fut.poll().map_err(|_| format_err!("during upgrade").compat().into()),
            Done => panic!("Connection has already been resolved or rejected"),
        }
    }

    fn next_state(&mut self) -> bool {
        use self::Connection::*;
        match mem::replace(self, Done) {
            Http(conn) => match conn.try_into_parts() {
                Some(parts) => {
                    trace!("transit to Upgrading");
                    *self = Upgrading(parts);
                    false
                }
                None => {
                    trace!("the connection is h2");
                    true
                }
            },
            Upgrading(parts) => {
                trace!("construct a future and transit to Upgrade");

                let Parts {
                    service, io, read_buf, ..
                } = parts;

                *self = Upgrade(service.upgrade(io, read_buf));

                false
            }
            Upgrade(..) => true,
            Done => unreachable!(),
        }
    }
}
