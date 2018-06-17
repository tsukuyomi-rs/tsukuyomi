//! [unstable]
//! Components for supporting for modifiers.

#![allow(missing_docs)]

use std::fmt;

use error::Error;
use future::{Future, Poll};
use input::Input;
use output::Output;

pub trait Modifier {
    fn before_handle(&self, input: &mut Input) -> BeforeHandle;
    fn after_handle(&self, input: &mut Input, output: Output) -> AfterHandle;
}

// ==== BeforeHandle ====

enum BeforeHandleState {
    Ready(Option<Result<(), Error>>),
    Async(Box<Future<Output = Result<(), Error>> + Send>),
}

impl fmt::Debug for BeforeHandleState {
    fn fmt(&self, f: &mut fmt::Formatter) -> fmt::Result {
        use self::BeforeHandleState::*;
        match *self {
            Ready(ref res) => f.debug_tuple("Ready").field(res).finish(),
            Async(..) => f.debug_tuple("Async").finish(),
        }
    }
}

#[derive(Debug)]
pub struct BeforeHandle(BeforeHandleState);

impl BeforeHandle {
    pub fn ready(res: Result<(), Error>) -> BeforeHandle {
        BeforeHandle(BeforeHandleState::Ready(Some(res)))
    }

    pub fn async<F>(future: F) -> BeforeHandle
    where
        F: Future<Output = Result<(), Error>> + Send + 'static,
    {
        BeforeHandle(BeforeHandleState::Async(Box::new(future)))
    }

    pub fn poll_ready(&mut self, input: &mut Input) -> Poll<Result<(), Error>> {
        use self::BeforeHandleState::*;
        match self.0 {
            Ready(ref mut res) => Poll::Ready(res.take().expect("BeforeHandle has already polled")),
            Async(ref mut f) => input.with_set(|| f.poll()),
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
        BeforeHandle::async(future)
    }
}

// ==== AfterHandle ====

enum AfterHandleState {
    Ready(Option<Result<Output, Error>>),
    Async(Box<Future<Output = Result<Output, Error>> + Send>),
}

impl fmt::Debug for AfterHandleState {
    fn fmt(&self, f: &mut fmt::Formatter) -> fmt::Result {
        use self::AfterHandleState::*;
        match *self {
            Ready(ref res) => f.debug_tuple("Immediate").field(res).finish(),
            Async(..) => f.debug_tuple("Boxed").finish(),
        }
    }
}

#[derive(Debug)]
pub struct AfterHandle(AfterHandleState);

impl AfterHandle {
    pub fn ready(res: Result<Output, Error>) -> AfterHandle {
        AfterHandle(AfterHandleState::Ready(Some(res)))
    }

    pub fn async<F>(future: F) -> AfterHandle
    where
        F: Future<Output = Result<Output, Error>> + Send + 'static,
    {
        AfterHandle(AfterHandleState::Async(Box::new(future)))
    }

    pub fn poll_ready(&mut self, input: &mut Input) -> Poll<Result<Output, Error>> {
        use self::AfterHandleState::*;
        match self.0 {
            Ready(ref mut res) => Poll::Ready(res.take().expect("AfterHandle has already polled")),
            Async(ref mut f) => input.with_set(|| f.poll()),
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
        AfterHandle::async(future)
    }
}
