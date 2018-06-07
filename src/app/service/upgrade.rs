use bytes::Bytes;
use failure::Error;
use futures::future::poll_fn;
use futures::sync::mpsc;
use futures::{Async, Future, Poll, Stream};

use context::Context;
use transport::Io;
use upgrade::UpgradeFn;

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

    pub fn sender(&self) -> Sender {
        let tx = self.tx.as_ref().unwrap().clone();
        Sender { tx: tx }
    }
}

#[derive(Debug)]
pub struct Sender {
    tx: mpsc::UnboundedSender<(UpgradeFn, Context)>,
}

impl Sender {
    pub(crate) fn send(&self, val: (UpgradeFn, Context)) {
        let _ = self.tx.unbounded_send(val);
    }
}
