use bytes::Bytes;
use futures::sync::mpsc;
use futures::{Async, Future, Poll, Stream};
use http::Request;

use server::Io;

use super::{BoxedUpgradeHandler, UpgradeContext};
use error::CritError;

// TODO: optimize

pub fn new() -> Receiver {
    let (tx, rx) = mpsc::unbounded();
    Receiver {
        tx: Some(tx),
        rx: rx,
        upgrade: None,
    }
}

#[derive(Debug)]
pub struct Receiver {
    tx: Option<mpsc::UnboundedSender<(BoxedUpgradeHandler, Request<()>)>>,
    rx: mpsc::UnboundedReceiver<(BoxedUpgradeHandler, Request<()>)>,
    upgrade: Option<(BoxedUpgradeHandler, Request<()>)>,
}

impl Receiver {
    pub fn sender(&self) -> Sender {
        let tx = self.tx.as_ref().unwrap().clone();
        Sender { tx: tx }
    }

    pub fn poll_ready(&mut self) -> Poll<(), CritError> {
        self.tx.take().map(|tx| drop(tx));

        if let Some(upgrade) = try_ready!(self.rx.poll().map_err(|_| format_err!("during rx.poll()").compat())) {
            self.upgrade = Some(upgrade);
        }

        Ok(Async::Ready(()))
    }

    pub fn try_upgrade(mut self, io: Io, read_buf: Bytes) -> Result<Box<Future<Item = (), Error = ()> + Send>, Io> {
        match self.upgrade.take() {
            Some((upgrade, request)) => {
                let cx = UpgradeContext {
                    io: io,
                    read_buf: read_buf,
                    request: request,
                    _priv: (),
                };
                Ok(Box::new(upgrade.upgrade(cx)))
            }
            None => Err(io),
        }
    }
}

#[derive(Debug)]
pub struct Sender {
    tx: mpsc::UnboundedSender<(BoxedUpgradeHandler, Request<()>)>,
}

impl Sender {
    pub fn send(&self, handler: BoxedUpgradeHandler, req: Request<()>) {
        let _ = self.tx.unbounded_send((handler, req));
    }
}
