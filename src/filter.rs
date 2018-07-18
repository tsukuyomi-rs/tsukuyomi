//! Definition of `Filter`

use futures::{Async, Future, Poll};
use std::fmt;
use std::sync::Arc;

use error::Error;
use input::Input;
use output::Output;

/// A trait representing a filter inserting before handlers.
pub trait Filter {
    /// Applies the filter to the specified request.
    fn apply(&self, input: &mut Input) -> Filtering;
}

impl<F> Filter for F
where
    F: Fn(&mut Input) -> Filtering,
{
    fn apply(&self, input: &mut Input) -> Filtering {
        (*self)(input)
    }
}

impl<T> Filter for Arc<T>
where
    T: Filter,
{
    fn apply(&self, input: &mut Input) -> Filtering {
        (**self).apply(input)
    }
}

/// An asynchronous value returned from `Filter`.
pub struct Filtering(FilteringKind);

enum FilteringKind {
    Ready(Option<Result<Option<Output>, Error>>),
    Async(Box<dyn Future<Item = Option<Output>, Error = Error> + Send + 'static>),
}

impl fmt::Debug for Filtering {
    fn fmt(&self, f: &mut fmt::Formatter) -> fmt::Result {
        match self.0 {
            FilteringKind::Ready(ref res) => f.debug_tuple("Ready").field(res).finish(),
            FilteringKind::Async(..) => f.debug_tuple("Async").finish(),
        }
    }
}

impl Filtering {
    /// Creates a `Filtering` from an immediately value.
    pub fn ready(result: Result<Option<Output>, Error>) -> Filtering {
        Filtering(FilteringKind::Ready(Some(result)))
    }

    /// Creates a `Filtering` from a `Future`.
    pub fn wrap_future(future: impl Future<Item = Option<Output>, Error = Error> + Send + 'static) -> Filtering {
        Filtering(FilteringKind::Async(Box::new(future)))
    }

    pub(crate) fn poll_ready(&mut self, input: &mut Input) -> Poll<Option<Output>, Error> {
        match self.0 {
            FilteringKind::Ready(ref mut res) => res.take().expect("This future has already polled").map(Async::Ready),
            FilteringKind::Async(ref mut f) => input.with_set_current(|| f.poll()),
        }
    }
}
