#![doc(html_root_url = "https://docs.rs/tsukuyomi-service/0.1.0-dev")]
#![deny(
    missing_debug_implementations,
    nonstandard_style,
    rust_2018_idioms,
    rust_2018_compatibility,
    unused
)]
#![forbid(clippy::unimplemented)]

use futures::{Future, IntoFuture};

#[doc(no_inline)]
pub use tower_service::Service;

/// A trait representing a factory of `Service`s.
///
/// The signature of this trait imitates `tower_util::MakeService` and will be replaced to it.
pub trait MakeService<Ctx, Request> {
    type Response;
    type Error;
    type Service: Service<Request, Response = Self::Response, Error = Self::Error>;
    type MakeError;
    type Future: Future<Item = Self::Service, Error = Self::MakeError>;

    fn make_service(&self, ctx: Ctx) -> Self::Future;
}

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

/// A trait representing the modification of `Service` to another one.
pub trait ModifyService<Ctx, Request, S> {
    type Response;
    type Error;
    type Service: Service<Request, Response = Self::Response, Error = Self::Error>;
    type ModifyError;
    type Future: Future<Item = Self::Service, Error = Self::ModifyError>;

    fn modify(&self, input: S, ctx: Ctx) -> Self::Future;
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
    fn modify(&self, input: S, _: Ctx) -> Self::Future {
        futures::future::ok(input)
    }
}

pub trait ModifyServiceRef<Ctx, Request, S> {
    type Response;
    type Error;
    type Service: Service<Request, Response = Self::Response, Error = Self::Error>;
    type ModifyError;
    type Future: Future<Item = Self::Service, Error = Self::ModifyError>;

    fn modify_ref(&self, input: S, ctx: &Ctx) -> Self::Future;
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

    fn modify_ref(&self, input: SvcIn, ctx: &Ctx) -> Self::Future {
        ModifyService::modify(self, input, ctx)
    }
}

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
        fn modify(&self, input: S, ctx: Ctx) -> Self::Future {
            (self.0)(input, ctx).into_future()
        }
    }

    ModifyServiceFn(f)
}

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
        fn modify(&self, input: S, ctx: &'a Ctx) -> Self::Future {
            (self.0)(input, ctx).into_future()
        }
    }

    ModifyServiceRefFn(f)
}

pub trait IntoMakeService<Ctx, Request> {
    type Response;
    type Error;
    type Service: Service<Request, Response = Self::Response, Error = Self::Error>;
    type MakeError;
    type MakeFuture: Future<Item = Self::Service, Error = Self::MakeError>;
    type MakeService: MakeService<
        Ctx, //
        Request,
        Response = Self::Response,
        Error = Self::Error,
        Service = Self::Service,
        MakeError = Self::MakeError,
        Future = Self::MakeFuture,
    >;

    fn into_make_service(self) -> Self::MakeService;
}

pub trait IntoMakeServiceRef<Ctx, Request> {
    type Response;
    type Error;
    type Service: Service<Request, Response = Self::Response, Error = Self::Error>;
    type MakeError;
    type MakeFuture: Future<Item = Self::Service, Error = Self::MakeError>;
    type MakeServiceRef: MakeServiceRef<
        Ctx, //
        Request,
        Response = Self::Response,
        Error = Self::Error,
        Service = Self::Service,
        MakeError = Self::MakeError,
        Future = Self::MakeFuture,
    >;

    fn into_make_service_ref(self) -> Self::MakeServiceRef;
}

impl<F, Ctx, Req, Res, E, Svc, MkErr, MkFut, MkSvc> IntoMakeServiceRef<Ctx, Req> for F
where
    for<'a> F: IntoMakeService<
        &'a Ctx,
        Req,
        Response = Res,
        Error = E,
        Service = Svc,
        MakeError = MkErr,
        MakeFuture = MkFut,
        MakeService = MkSvc,
    >,
    for<'a> MkSvc: MakeService<
        &'a Ctx,
        Req,
        Response = Res,
        Error = E,
        Service = Svc,
        MakeError = MkErr,
        Future = MkFut,
    >,
    Svc: Service<Req, Response = Res, Error = E>,
    MkFut: Future<Item = Svc, Error = MkErr>,
{
    type Response = Res;
    type Error = E;
    type Service = Svc;
    type MakeError = MkErr;
    type MakeFuture = MkFut;
    type MakeServiceRef = MkSvc;

    #[inline]
    fn into_make_service_ref(self) -> Self::MakeServiceRef {
        IntoMakeService::into_make_service(self)
    }
}
