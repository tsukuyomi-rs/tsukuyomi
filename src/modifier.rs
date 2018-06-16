//! [unstable]
//! Components for supporting for modifiers.

#![allow(missing_docs)]

use std::fmt;

use error::Error;
use future::{Future, Poll};
use output::Output;

pub trait Modifier {
    fn before_handle(&self) -> BeforeHandle;
    fn after_handle(&self, output: Output) -> AfterHandle;
}

// ==== BeforeHandle ====

enum BeforeHandleState {
    Immediate(Option<Result<(), Error>>),
    Boxed(Box<Future<Output = Result<(), Error>> + Send>),
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
    pub fn immediate(res: Result<(), Error>) -> BeforeHandle {
        BeforeHandle(BeforeHandleState::Immediate(Some(res)))
    }

    pub fn boxed<F>(future: F) -> BeforeHandle
    where
        F: Future<Output = Result<(), Error>> + Send + 'static,
    {
        BeforeHandle(BeforeHandleState::Boxed(Box::new(future)))
    }

    pub fn poll_ready(&mut self) -> Poll<Result<(), Error>> {
        use self::BeforeHandleState::*;
        match self.0 {
            Immediate(ref mut res) => Poll::Ready(res.take().expect("BeforeHandle has already polled")),
            Boxed(ref mut f) => f.poll(),
        }
    }
}

// impl From<Result<Input, (Input, Error)>> for BeforeHandle {
//     fn from(res: Result<Input, (Input, Error)>) -> BeforeHandle {
//         BeforeHandle::immediate(res)
//     }
// }

impl<F> From<F> for BeforeHandle
where
    F: Future<Output = Result<(), Error>> + Send + 'static,
{
    fn from(future: F) -> BeforeHandle {
        BeforeHandle::boxed(future)
    }
}

// ==== AfterHandle ====

enum AfterHandleState {
    Immediate(Option<Result<Output, Error>>),
    Boxed(Box<Future<Output = Result<Output, Error>> + Send>),
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
        F: Future<Output = Result<Output, Error>> + Send + 'static,
    {
        AfterHandle(AfterHandleState::Boxed(Box::new(future)))
    }

    pub fn poll_ready(&mut self) -> Poll<Result<Output, Error>> {
        use self::AfterHandleState::*;
        match self.0 {
            Immediate(ref mut res) => Poll::Ready(res.take().expect("AfterHandle has already polled")),
            Boxed(ref mut f) => f.poll(),
        }
    }
}

// impl From<Result<Input, (Input, Error)>> for AfterHandle {
//     fn from(res: Result<Input, (Input, Error)>) -> AfterHandle {
//         AfterHandle::immediate(res)
//     }
// }

impl<F> From<F> for AfterHandle
where
    F: Future<Output = Result<Output, Error>> + Send + 'static,
{
    fn from(future: F) -> AfterHandle {
        AfterHandle::boxed(future)
    }
}
