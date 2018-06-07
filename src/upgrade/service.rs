use bytes::Bytes;
use failure::Error;
use futures::future::poll_fn;
use futures::sync::mpsc;
use futures::{Async, Future, Poll, Stream};
use std::fmt;

use context::Context;
use transport::Io;

use super::UpgradeHandler;

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
    tx: Option<mpsc::UnboundedSender<(UpgradeFn, Context)>>,
    rx: mpsc::UnboundedReceiver<(UpgradeFn, Context)>,
    upgrade: Option<(UpgradeFn, Context)>,
}

impl Receiver {
    pub fn sender(&self) -> Sender {
        let tx = self.tx.as_ref().unwrap().clone();
        Sender { tx: tx }
    }

    pub fn poll_ready(&mut self) -> Poll<(), Error> {
        self.tx.take().map(|tx| drop(tx));

        if let Some(upgrade) =
            try_ready!(self.rx.poll().map_err(|_| format_err!("during rx.poll()")))
        {
            self.upgrade = Some(upgrade);
        }

        Ok(Async::Ready(()))
    }

    pub fn upgrade(
        mut self,
        io: Io,
        read_buf: Bytes,
    ) -> Result<Box<Future<Item = (), Error = ()> + Send>, (Io, Bytes)> {
        match self.upgrade.take() {
            Some((mut upgrade, cx)) => {
                let mut upgraded = upgrade.upgrade(io, read_buf, &cx);
                Ok(Box::new(poll_fn(move || cx.set(|| upgraded.poll()))))
            }
            None => Err((io, read_buf)),
        }
    }
}

#[derive(Debug)]
pub struct Sender {
    tx: mpsc::UnboundedSender<(UpgradeFn, Context)>,
}

impl Sender {
    pub fn send(&self, val: (UpgradeFn, Context)) {
        let _ = self.tx.unbounded_send(val);
    }
}

// ==== UpgradeFn

pub struct UpgradeFn {
    inner: Box<
        FnMut(Io, Bytes, &Context) -> Box<Future<Item = (), Error = ()> + Send> + Send + 'static,
    >,
}

impl fmt::Debug for UpgradeFn {
    fn fmt(&self, f: &mut fmt::Formatter) -> fmt::Result {
        f.debug_struct("UpgradeFn").finish()
    }
}

impl<H> From<H> for UpgradeFn
where
    H: UpgradeHandler + Send + 'static,
    H::Future: Send + 'static,
{
    fn from(handler: H) -> Self {
        let mut handler = Some(handler);
        UpgradeFn {
            inner: Box::new(move |io, read_buf, cx| {
                let handler = handler.take().expect("cannot upgrade twice");
                Box::new(handler.upgrade(io, read_buf, cx))
            }),
        }
    }
}

impl UpgradeFn {
    pub fn upgrade(
        &mut self,
        io: Io,
        read_buf: Bytes,
        cx: &Context,
    ) -> Box<Future<Item = (), Error = ()> + Send + 'static> {
        (self.inner)(io, read_buf, cx)
    }
}
