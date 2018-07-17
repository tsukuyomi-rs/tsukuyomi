#![allow(missing_docs)]

use futures::{Async, Future, Poll};
use std::fmt;
use std::sync::Arc;

use error::Error;
use input::Input;
use output::Output;

pub trait PipelineHandler {
    fn handle(&self, input: &mut Input) -> Pipeline;
}

impl<F> PipelineHandler for F
where
    F: Fn(&mut Input) -> Pipeline,
{
    fn handle(&self, input: &mut Input) -> Pipeline {
        (*self)(input)
    }
}

impl<T> PipelineHandler for Arc<T>
where
    T: PipelineHandler,
{
    fn handle(&self, input: &mut Input) -> Pipeline {
        (**self).handle(input)
    }
}

pub struct Pipeline(PipelineKind);

enum PipelineKind {
    Ready(Option<Result<Option<Output>, Error>>),
    Async(Box<dyn Future<Item = Option<Output>, Error = Error> + Send + 'static>),
}

impl fmt::Debug for Pipeline {
    fn fmt(&self, f: &mut fmt::Formatter) -> fmt::Result {
        match self.0 {
            PipelineKind::Ready(ref res) => f.debug_tuple("Ready").field(res).finish(),
            PipelineKind::Async(..) => f.debug_tuple("Async").finish(),
        }
    }
}

impl Pipeline {
    pub fn ready(result: Result<Option<Output>, Error>) -> Pipeline {
        Pipeline(PipelineKind::Ready(Some(result)))
    }

    pub fn wrap_future(future: impl Future<Item = Option<Output>, Error = Error> + Send + 'static) -> Pipeline {
        Pipeline(PipelineKind::Async(Box::new(future)))
    }

    pub(crate) fn poll_ready(&mut self, input: &mut Input) -> Poll<Option<Output>, Error> {
        match self.0 {
            PipelineKind::Ready(ref mut res) => res.take().expect("This future has already polled").map(Async::Ready),
            PipelineKind::Async(ref mut f) => input.with_set_current(|| f.poll()),
        }
    }
}
