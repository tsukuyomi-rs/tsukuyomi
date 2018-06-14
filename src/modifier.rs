//! [unstable]
//! Components for supporting for modifiers.

#![allow(missing_docs)]

use futures::Future;
use std::fmt;

use context::Context;
use error::Error;
use future::Poll;
use output::Output;

pub trait Modifier {
    fn before_handle(&self, cx: Context) -> BeforeHandle;
    fn after_handle(&self, cx: &Context, output: Output) -> AfterHandle;
}

// ==== BeforeHandle ====

enum BeforeHandleState {
    Immediate(Option<Result<Context, (Context, Error)>>),
    Boxed(Box<Future<Item = Context, Error = (Context, Error)> + Send>),
}

impl fmt::Debug for BeforeHandleState {
    fn fmt(&self, f: &mut fmt::Formatter) -> fmt::Result {
        use self::BeforeHandleState::*;
        match *self {
            Immediate(ref res) => f.debug_tuple("Immediate").field(res).finish(),
            Boxed(..) => f.debug_tuple("Boxed").finish(),
        }
    }
}

#[derive(Debug)]
pub struct BeforeHandle(BeforeHandleState);

impl BeforeHandle {
    pub fn immediate(res: Result<Context, (Context, Error)>) -> BeforeHandle {
        BeforeHandle(BeforeHandleState::Immediate(Some(res)))
    }

    pub fn boxed<F>(future: F) -> BeforeHandle
    where
        F: Future<Item = Context, Error = (Context, Error)> + Send + 'static,
    {
        BeforeHandle(BeforeHandleState::Boxed(Box::new(future)))
    }

    pub fn poll_ready(&mut self) -> Poll<Result<Context, (Context, Error)>> {
        use self::BeforeHandleState::*;
        match self.0 {
            Immediate(ref mut res) => Poll::Ready(res.take().expect("BeforeHandle has already polled")),
            Boxed(ref mut f) => f.poll().into(),
        }
    }
}

// impl From<Result<Context, (Context, Error)>> for BeforeHandle {
//     fn from(res: Result<Context, (Context, Error)>) -> BeforeHandle {
//         BeforeHandle::immediate(res)
//     }
// }

impl<F> From<F> for BeforeHandle
where
    F: Future<Item = Context, Error = (Context, Error)> + Send + 'static,
{
    fn from(future: F) -> BeforeHandle {
        BeforeHandle::boxed(future)
    }
}

// ==== AfterHandle ====

enum AfterHandleState {
    Immediate(Option<Result<Output, Error>>),
    Boxed(Box<Future<Item = Output, Error = Error> + Send>),
}

impl fmt::Debug for AfterHandleState {
    fn fmt(&self, f: &mut fmt::Formatter) -> fmt::Result {
        use self::AfterHandleState::*;
        match *self {
            Immediate(ref res) => f.debug_tuple("Immediate").field(res).finish(),
            Boxed(..) => f.debug_tuple("Boxed").finish(),
        }
    }
}

#[derive(Debug)]
pub struct AfterHandle(AfterHandleState);

impl AfterHandle {
    pub fn immediate(res: Result<Output, Error>) -> AfterHandle {
        AfterHandle(AfterHandleState::Immediate(Some(res)))
    }

    pub fn boxed<F>(future: F) -> AfterHandle
    where
        F: Future<Item = Output, Error = Error> + Send + 'static,
    {
        AfterHandle(AfterHandleState::Boxed(Box::new(future)))
    }

    pub fn poll_ready(&mut self) -> Poll<Result<Output, Error>> {
        use self::AfterHandleState::*;
        match self.0 {
            Immediate(ref mut res) => Poll::Ready(res.take().expect("AfterHandle has already polled")),
            Boxed(ref mut f) => f.poll().into(),
        }
    }
}

// impl From<Result<Context, (Context, Error)>> for AfterHandle {
//     fn from(res: Result<Context, (Context, Error)>) -> AfterHandle {
//         AfterHandle::immediate(res)
//     }
// }

impl<F> From<F> for AfterHandle
where
    F: Future<Item = Output, Error = Error> + Send + 'static,
{
    fn from(future: F) -> AfterHandle {
        AfterHandle::boxed(future)
    }
}
