use {
    super::{
        generic::{Combine, Func, Tuple},
        Extract, ExtractStatus, Extractor,
    },
    crate::{common::Never, error::Error, input::Input},
    futures::{future, Async, Future, IntoFuture, Poll},
};

#[derive(Debug)]
pub struct Builder<E> {
    extractor: E,
}

#[cfg_attr(feature = "cargo-clippy", allow(use_self))]
impl<E> Builder<E>
where
    E: Extractor,
{
    /// Creates a `Builder` from the specified extractor.
    #[inline]
    pub fn new(extractor: E) -> Self {
        Self { extractor }
    }

    /// Returns the inner extractor.
    #[inline]
    pub fn into_inner(self) -> E {
        self.extractor
    }

    pub fn optional<T>(self) -> Builder<impl Extractor<Output = (Option<T>,), Error = Never>>
    where
        E: Extractor<Output = (T,)>,
        T: 'static,
    {
        Builder {
            extractor: super::raw(move |input| {
                self.extractor
                    .extract(input)
                    .map(|status| {
                        status
                            .map_ready(|(out,)| (Some(out),))
                            .map_pending(|mut future| {
                                futures::future::poll_fn(move || {
                                    future
                                        .poll()
                                        .map(|x| x.map(|(out,)| (Some(out),)))
                                        .or_else(|_| Ok(Async::Ready((None,))))
                                })
                            })
                    }).or_else(|_| Ok(ExtractStatus::Ready((None,))))
            }),
        }
    }

    pub fn fallible<T>(
        self,
    ) -> Builder<impl Extractor<Output = (Result<T, E::Error>,), Error = Never>>
    where
        E: Extractor<Output = (T,)>,
        T: 'static,
    {
        Builder {
            extractor: super::raw(move |input| {
                self.extractor
                    .extract(input)
                    .map(|status| {
                        status
                            .map_ready(|(out,)| (Ok(out),))
                            .map_pending(|mut future| {
                                futures::future::poll_fn(move || {
                                    future
                                        .poll()
                                        .map(|x| x.map(|(out,)| (Ok(out),)))
                                        .or_else(|err| Ok(Async::Ready((Err(err),))))
                                })
                            })
                    }).or_else(|err| Ok(ExtractStatus::Ready((Err(err),))))
            }),
        }
    }

    pub fn and<T>(
        self,
        other: T,
    ) -> Builder<impl Extractor<Output = <E::Output as Combine<T::Output>>::Out, Error = Error>>
    where
        T: Extractor,
        E::Output: Combine<T::Output> + Send + 'static,
        T::Output: Send + 'static,
    {
        #[allow(missing_debug_implementations)]
        struct AndFuture<L: Future, R: Future> {
            left: MaybeDone<L>,
            right: MaybeDone<R>,
        }

        impl<L: Future, R: Future> AndFuture<L, R>
        where
            L::Error: Into<Error>,
            R::Error: Into<Error>,
            L::Item: Tuple + Combine<R::Item>,
            R::Item: Tuple,
        {
            fn poll_ready(&mut self) -> Poll<(), Error> {
                futures::try_ready!(self.left.poll_ready().map_err(Into::into));
                futures::try_ready!(self.right.poll_ready().map_err(Into::into));
                Ok(Async::Ready(()))
            }
        }

        impl<L: Future, R: Future> Future for AndFuture<L, R>
        where
            L::Error: Into<Error>,
            R::Error: Into<Error>,
            L::Item: Tuple + Combine<R::Item>,
            R::Item: Tuple,
        {
            type Item = <L::Item as Combine<R::Item>>::Out;
            type Error = Error;

            fn poll(&mut self) -> Poll<Self::Item, Self::Error> {
                match self.poll_ready() {
                    Ok(Async::Ready(())) => {
                        let left = self.left.take_item().expect("the item should be available");
                        let right = self
                            .right
                            .take_item()
                            .expect("the item should be available");
                        Ok(Async::Ready(left.combine(right)))
                    }
                    Ok(Async::NotReady) => Ok(Async::NotReady),
                    Err(err) => {
                        let _ = self.left.take_item();
                        let _ = self.right.take_item();
                        Err(err)
                    }
                }
            }
        }

        let left = self.extractor;
        let right = other;
        Builder {
            extractor: super::raw(move |input| {
                let left = match left.extract(input).map_err(Into::into)? {
                    ExtractStatus::Ready(output) => MaybeDone::Ready(output),
                    ExtractStatus::Pending(future) => MaybeDone::Pending(future),
                    ExtractStatus::Canceled(output) => return Ok(ExtractStatus::Canceled(output)),
                };
                let right = match right.extract(input).map_err(Into::into)? {
                    ExtractStatus::Ready(output) => MaybeDone::Ready(output),
                    ExtractStatus::Pending(future) => MaybeDone::Pending(future),
                    ExtractStatus::Canceled(output) => return Ok(ExtractStatus::Canceled(output)),
                };
                match (left, right) {
                    (MaybeDone::Ready(left), MaybeDone::Ready(right)) => {
                        Ok(ExtractStatus::Ready(left.combine(right)))
                    }
                    (left, right) => Ok(ExtractStatus::Pending(AndFuture { left, right })),
                }
            }),
        }
    }

    pub fn or<T>(self, other: T) -> Builder<impl Extractor<Output = E::Output, Error = Error>>
    where
        T: Extractor<Output = E::Output>,
    {
        #[allow(missing_debug_implementations)]
        #[cfg_attr(feature = "cargo-clippy", allow(type_complexity))]
        enum OrFuture<L, R>
        where
            L: Future,
            R: Future<Item = L::Item>,
            L::Error: Into<Error>,
            R::Error: Into<Error>,
        {
            Left(L),
            Right(R),
            Both(
                future::Select<
                    future::MapErr<L, fn(L::Error) -> Error>,
                    future::MapErr<R, fn(R::Error) -> Error>,
                >,
            ),
        }

        impl<L, R> Future for OrFuture<L, R>
        where
            L: Future,
            R: Future<Item = L::Item>,
            L::Error: Into<Error>,
            R::Error: Into<Error>,
        {
            type Item = L::Item;
            type Error = Error;

            fn poll(&mut self) -> Poll<Self::Item, Self::Error> {
                match self {
                    OrFuture::Both(ref mut future) => future
                        .poll()
                        .map(|x| x.map(|(out, _next)| out))
                        .map_err(|(err, _next)| err),
                    OrFuture::Left(ref mut left) => left.poll().map_err(Into::into),
                    OrFuture::Right(ref mut right) => right.poll().map_err(Into::into),
                }
            }
        }

        let left = self.extractor;
        let right = other;
        Builder {
            extractor: super::raw(move |input| {
                let left_status = match left.extract(input) {
                    Ok(status) => status,
                    Err(..) => {
                        return right
                            .extract(input)
                            .map(|status| status.map_pending(OrFuture::Right))
                            .map_err(Into::into)
                    }
                };

                let left = match left_status {
                    status @ ExtractStatus::Ready(..) | status @ ExtractStatus::Canceled(..) => {
                        return Ok(status.map_pending(|_| unreachable!()));
                    }
                    ExtractStatus::Pending(left) => left,
                };

                match right.extract(input) {
                    Ok(status) => Ok(status.map_pending(|right| {
                        OrFuture::Both(
                            left.map_err(Into::into as fn(E::Error) -> Error)
                                .select(right.map_err(Into::into as fn(T::Error) -> Error)),
                        )
                    })),
                    Err(..) => Ok(ExtractStatus::Pending(OrFuture::Left(left))),
                }
            }),
        }
    }

    pub fn map<F>(self, f: F) -> Builder<impl Extractor<Output = (F::Out,), Error = E::Error>>
    where
        F: Func<E::Output> + Clone + Send + Sync + 'static,
    {
        Builder {
            extractor: super::raw(move |input| {
                self.extractor
                    .extract(input) //
                    .map(|status| {
                        status
                            .map_ready(|args| (f.call(args),))
                            .map_pending(|mut future| {
                                let f = f.clone();
                                futures::future::poll_fn(move || {
                                    future.poll().map(|x| x.map(|out| (f.call(out),)))
                                })
                            })
                    })
            }),
        }
    }

    pub fn and_then<F, R>(self, f: F) -> Builder<impl Extractor<Output = (R::Item,), Error = Error>>
    where
        F: Func<E::Output, Out = R> + Clone + Send + Sync + 'static,
        R: IntoFuture + 'static,
        R::Future: Send + 'static,
        R::Error: Into<Error>,
    {
        #[allow(missing_debug_implementations)]
        enum AndThenState<F1, F2, F> {
            First(F1, F),
            Second(F2),
            Empty,
        }

        Builder {
            extractor: super::raw(move |input| {
                let mut state = match self.extractor.extract(input).map_err(Into::into)? {
                    ExtractStatus::Canceled(output) => return Ok(ExtractStatus::Canceled(output)),
                    ExtractStatus::Ready(arg) => {
                        let future = f.call(arg).into_future();
                        AndThenState::Second(future)
                    }
                    ExtractStatus::Pending(future) => AndThenState::First(future, f.clone()),
                };
                Ok(ExtractStatus::Pending(futures::future::poll_fn(
                    move || loop {
                        let next_future = match state {
                            AndThenState::First(ref mut f1, ref f) => match f1.poll() {
                                Ok(Async::NotReady) => return Ok(Async::NotReady),
                                Ok(Async::Ready(ok)) => Ok(f.call(ok)),
                                Err(err) => Err(err),
                            },
                            AndThenState::Second(ref mut f2) => {
                                return f2.poll().map(|x| x.map(|out| (out,))).map_err(Into::into)
                            }
                            AndThenState::Empty => panic!("This future has already polled."),
                        };

                        match next_future {
                            Ok(future) => {
                                state = AndThenState::Second(future.into_future());
                                continue;
                            }
                            Err(err) => {
                                state = AndThenState::Empty;
                                return Err(err.into());
                            }
                        }
                    },
                )))
            }),
        }
    }
}

impl<E> Extractor for Builder<E>
where
    E: Extractor,
{
    type Output = E::Output;
    type Error = E::Error;
    type Future = E::Future;

    #[inline]
    fn extract(&self, input: &mut Input<'_>) -> Extract<Self> {
        self.extractor.extract(input)
    }
}

#[allow(missing_debug_implementations)]
enum MaybeDone<F: Future> {
    Ready(F::Item),
    Pending(F),
    Gone,
}

impl<F: Future> MaybeDone<F> {
    fn poll_ready(&mut self) -> Poll<(), F::Error> {
        let async_ = match self {
            MaybeDone::Ready(..) => return Ok(Async::Ready(())),
            MaybeDone::Pending(ref mut future) => future.poll()?,
            MaybeDone::Gone => panic!("This future has already polled"),
        };
        match async_ {
            Async::Ready(item) => {
                *self = MaybeDone::Ready(item);
                Ok(Async::Ready(()))
            }
            Async::NotReady => Ok(Async::NotReady),
        }
    }

    fn take_item(&mut self) -> Option<F::Item> {
        match std::mem::replace(self, MaybeDone::Gone) {
            MaybeDone::Ready(item) => Some(item),
            _ => None,
        }
    }
}
