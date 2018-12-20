//! Abstraction of service layer, based on [`tower-service`].
//!
//! [`tower-service`]: https://crates.io/crates/tower-service

#![doc(html_root_url = "https://docs.rs/tsukuyomi-service/0.1.0-dev")]
#![deny(
    missing_docs,
    missing_debug_implementations,
    nonstandard_style,
    rust_2018_idioms,
    rust_2018_compatibility,
    unused
)]
#![forbid(clippy::unimplemented)]

use futures::{Async, Future, IntoFuture, Poll};

#[doc(no_inline)]
pub use tower_service::Service;

/// Creates a `Service` from a function.
pub fn service_fn<Request, R>(
    f: impl FnMut(Request) -> R,
) -> impl Service<
    Request, //
    Response = R::Item,
    Error = R::Error,
    Future = R::Future,
>
where
    R: IntoFuture,
{
    #[allow(missing_debug_implementations)]
    struct ServiceFn<F>(F);

    impl<F, Request, R> Service<Request> for ServiceFn<F>
    where
        F: FnMut(Request) -> R,
        R: IntoFuture,
    {
        type Response = R::Item;
        type Error = R::Error;
        type Future = R::Future;

        #[inline]
        fn poll_ready(&mut self) -> Poll<(), Self::Error> {
            Ok(Async::Ready(()))
        }

        #[inline]
        fn call(&mut self, request: Request) -> Self::Future {
            (self.0)(request).into_future()
        }
    }

    ServiceFn(f)
}

/// A trait representing a factory of `Service`s.
///
/// The signature of this trait imitates `tower_util::MakeService`,
/// but there are the following differences:
///
/// * This trait does not have the method `poll_ready` to check
///   if the factory is ready for creating a `Service`.
/// * The method `make_service` is *immutable*.
pub trait MakeService<Ctx, Request> {
    /// The response type returned by `Service`.
    type Response;
    /// The error type returned by `Service`.
    type Error;
    /// The type of services created by this factory.
    type Service: Service<Request, Response = Self::Response, Error = Self::Error>;
    /// The type of errors that occur while creating `Service`.
    type MakeError;
    /// The type of `Future` returned from `make_service`.
    type Future: Future<Item = Self::Service, Error = Self::MakeError>;

    /// Creates a `Future` that will return a value of `Service`.
    fn make_service(&self, ctx: Ctx) -> Self::Future;
}

/// An *alias* of `MakeService` receiving the context value of `Ctx` as reference.
#[allow(missing_docs)]
pub trait MakeServiceRef<Ctx, Request> {
    type Response;
    type Error;
    type Service: Service<Request, Response = Self::Response, Error = Self::Error>;
    type MakeError;
    type Future: Future<Item = Self::Service, Error = Self::MakeError>;

    fn make_service_ref(&self, ctx: &Ctx) -> Self::Future;
}

impl<S, T, Req, Res, Err, Svc, MkErr, Fut> MakeServiceRef<T, Req> for S
where
    for<'a> S: MakeService<
        &'a T,
        Req,
        Response = Res,
        Error = Err,
        Service = Svc,
        MakeError = MkErr,
        Future = Fut,
    >,
    Svc: Service<Req, Response = Res, Error = Err>,
    Fut: Future<Item = Svc, Error = MkErr>,
{
    type Response = Res;
    type Error = Err;
    type Service = Svc;
    type MakeError = MkErr;
    type Future = Fut;

    #[inline]
    fn make_service_ref(&self, ctx: &T) -> Self::Future {
        MakeService::make_service(self, ctx)
    }
}

/// Creates a `MakeService` from a function.
pub fn make_service<Request, Ctx, R>(
    f: impl Fn(Ctx) -> R,
) -> impl MakeService<
    Ctx, //
    Request,
    Response = <R::Item as Service<Request>>::Response,
    Error = <R::Item as Service<Request>>::Error,
    Service = R::Item,
    MakeError = R::Error,
    Future = R::Future,
>
where
    R: IntoFuture,
    R::Item: Service<Request>,
{
    #[allow(missing_debug_implementations)]
    struct MakeServiceFn<F>(F);

    impl<F, Request, Ctx, R> MakeService<Ctx, Request> for MakeServiceFn<F>
    where
        F: Fn(Ctx) -> R,
        R: IntoFuture,
        R::Item: Service<Request>,
    {
        type Response = <R::Item as Service<Request>>::Response;
        type Error = <R::Item as Service<Request>>::Error;
        type Service = R::Item;
        type MakeError = R::Error;
        type Future = R::Future;

        #[inline]
        fn make_service(&self, ctx: Ctx) -> Self::Future {
            (self.0)(ctx).into_future()
        }
    }

    MakeServiceFn(f)
}

/// Creates a `ModifyServiceRef` from a function.
pub fn make_service_ref<Request, Ctx, R>(
    f: impl Fn(&Ctx) -> R,
) -> impl for<'a> MakeService<
    &'a Ctx, //
    Request,
    Response = <R::Item as Service<Request>>::Response,
    Error = <R::Item as Service<Request>>::Error,
    Service = R::Item,
    MakeError = R::Error,
    Future = R::Future,
>
where
    R: IntoFuture,
    R::Item: Service<Request>,
{
    #[allow(missing_debug_implementations)]
    struct MakeServiceRefFn<F>(F);

    impl<'a, F, Request, Ctx, R> MakeService<&'a Ctx, Request> for MakeServiceRefFn<F>
    where
        F: Fn(&Ctx) -> R,
        R: IntoFuture,
        R::Item: Service<Request>,
    {
        type Response = <R::Item as Service<Request>>::Response;
        type Error = <R::Item as Service<Request>>::Error;
        type Service = R::Item;
        type MakeError = R::Error;
        type Future = R::Future;

        #[inline]
        fn make_service(&self, ctx: &'a Ctx) -> Self::Future {
            (self.0)(ctx).into_future()
        }
    }

    MakeServiceRefFn(f)
}

/// A trait representing the modification of `Service` to another one.
pub trait ModifyService<Ctx, Request, S> {
    /// The response type returned by the modified `Service`.
    type Response;
    /// The error type returned by the modified `Service`.
    type Error;
    /// The type of modified service.
    type Service: Service<Request, Response = Self::Response, Error = Self::Error>;
    /// The error that occurs when modifying services.
    type ModifyError;
    /// The type of `Future` returned from `modify_service`.
    type Future: Future<Item = Self::Service, Error = Self::ModifyError>;

    /// Modifies a service using the specified context.
    fn modify_service(&self, input: S, ctx: Ctx) -> Self::Future;
}

impl<Ctx, Request, S> ModifyService<Ctx, Request, S> for ()
where
    S: Service<Request>,
{
    type Response = S::Response;
    type Error = S::Error;
    type Service = S;
    type ModifyError = std::io::Error;
    type Future = futures::future::FutureResult<Self::Service, Self::ModifyError>;

    #[inline]
    fn modify_service(&self, input: S, _: Ctx) -> Self::Future {
        futures::future::ok(input)
    }
}

/// An *alias* of `ModifyService` receiving the context value of `Ctx` as reference.
#[allow(missing_docs)]
pub trait ModifyServiceRef<Ctx, Request, S> {
    type Response;
    type Error;
    type Service: Service<Request, Response = Self::Response, Error = Self::Error>;
    type ModifyError;
    type Future: Future<Item = Self::Service, Error = Self::ModifyError>;

    fn modify_service_ref(&self, input: S, ctx: &Ctx) -> Self::Future;
}

impl<M, SvcIn, SvcOut, Ctx, Req, Res, Err, ModErr, Fut> ModifyServiceRef<Ctx, Req, SvcIn> for M
where
    for<'a> M: ModifyService<
        &'a Ctx,
        Req,
        SvcIn,
        Response = Res,
        Error = Err,
        Service = SvcOut,
        ModifyError = ModErr,
        Future = Fut,
    >,
    SvcOut: Service<Req, Response = Res, Error = Err>,
    Fut: Future<Item = SvcOut, Error = ModErr>,
{
    type Response = Res;
    type Error = Err;
    type Service = SvcOut;
    type ModifyError = ModErr;
    type Future = Fut;

    fn modify_service_ref(&self, input: SvcIn, ctx: &Ctx) -> Self::Future {
        ModifyService::modify_service(self, input, ctx)
    }
}

/// Creates a `ModifyService` from a function.
pub fn modify_service<Request, S, Ctx, R>(
    f: impl Fn(S, Ctx) -> R,
) -> impl ModifyService<
    Ctx, //
    Request,
    S,
    Response = <R::Item as Service<Request>>::Response,
    Error = <R::Item as Service<Request>>::Error,
    Service = R::Item,
    ModifyError = R::Error,
    Future = R::Future,
>
where
    R: IntoFuture,
    R::Item: Service<Request>,
{
    #[allow(missing_debug_implementations)]
    struct ModifyServiceFn<F>(F);

    impl<F, Request, S, Ctx, R> ModifyService<Ctx, Request, S> for ModifyServiceFn<F>
    where
        F: Fn(S, Ctx) -> R,
        R: IntoFuture,
        R::Item: Service<Request>,
    {
        type Response = <R::Item as Service<Request>>::Response;
        type Error = <R::Item as Service<Request>>::Error;
        type Service = R::Item;
        type ModifyError = R::Error;
        type Future = R::Future;

        #[inline]
        fn modify_service(&self, input: S, ctx: Ctx) -> Self::Future {
            (self.0)(input, ctx).into_future()
        }
    }

    ModifyServiceFn(f)
}

/// Creates a `ModifyServiceRef` from a function.
pub fn modify_service_ref<Request, S, Ctx, R>(
    f: impl Fn(S, &Ctx) -> R,
) -> impl for<'a> ModifyService<
    &'a Ctx, //
    Request,
    S,
    Response = <R::Item as Service<Request>>::Response,
    Error = <R::Item as Service<Request>>::Error,
    Service = R::Item,
    ModifyError = R::Error,
    Future = R::Future,
>
where
    R: IntoFuture,
    R::Item: Service<Request>,
{
    #[allow(missing_debug_implementations)]
    struct ModifyServiceRefFn<F>(F);

    impl<'a, F, Request, S, Ctx, R> ModifyService<&'a Ctx, Request, S> for ModifyServiceRefFn<F>
    where
        F: Fn(S, &Ctx) -> R,
        R: IntoFuture,
        R::Item: Service<Request>,
    {
        type Response = <R::Item as Service<Request>>::Response;
        type Error = <R::Item as Service<Request>>::Error;
        type Service = R::Item;
        type ModifyError = R::Error;
        type Future = R::Future;

        #[inline]
        fn modify_service(&self, input: S, ctx: &'a Ctx) -> Self::Future {
            (self.0)(input, ctx).into_future()
        }
    }

    ModifyServiceRefFn(f)
}
