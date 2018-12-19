#![doc(html_root_url = "https://docs.rs/tsukuyomi-service/0.1.0-dev")]
#![deny(
    missing_debug_implementations,
    nonstandard_style,
    rust_2018_idioms,
    rust_2018_compatibility,
    unused
)]
#![forbid(clippy::unimplemented)]

use futures::Future;

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

pub trait ModifyService<Ctx, Request, S> {
    type Response;
    type Error;
    type Service: Service<Request, Response = Self::Response, Error = Self::Error>;

    fn modify(&self, input: S, ctx: Ctx) -> Self::Service;
}

impl<Ctx, Request, S> ModifyService<Ctx, Request, S> for ()
where
    S: Service<Request>,
{
    type Response = S::Response;
    type Error = S::Error;
    type Service = S;

    #[inline]
    fn modify(&self, input: S, _: Ctx) -> Self::Service {
        input
    }
}

pub trait ModifyServiceRef<Ctx, Request, S> {
    type Response;
    type Error;
    type Service: Service<Request, Response = Self::Response, Error = Self::Error>;

    fn modify_ref(&self, input: S, ctx: &Ctx) -> Self::Service;
}

impl<M, SvcIn, SvcOut, Ctx, Req, Res, Err> ModifyServiceRef<Ctx, Req, SvcIn> for M
where
    for<'a> M: ModifyService<&'a Ctx, Req, SvcIn, Response = Res, Error = Err, Service = SvcOut>,
    SvcOut: Service<Req, Response = Res, Error = Err>,
{
    type Response = Res;
    type Error = Err;
    type Service = SvcOut;

    fn modify_ref(&self, input: SvcIn, ctx: &Ctx) -> Self::Service {
        ModifyService::modify(self, input, ctx)
    }
}

pub fn modify_service<SvcIn, SvcOut, Request, Ctx>(
    f: impl Fn(SvcIn, Ctx) -> SvcOut,
) -> impl ModifyService<
    Ctx, //
    Request,
    SvcIn,
    Response = SvcOut::Response,
    Error = SvcOut::Error,
    Service = SvcOut,
>
where
    SvcOut: Service<Request>,
{
    #[allow(missing_debug_implementations)]
    struct ModifyServiceFn<F>(F);

    impl<F, SvcIn, SvcOut, Request, Ctx> ModifyService<Ctx, Request, SvcIn> for ModifyServiceFn<F>
    where
        F: Fn(SvcIn, Ctx) -> SvcOut,
        SvcOut: Service<Request>,
    {
        type Response = SvcOut::Response;
        type Error = SvcOut::Error;
        type Service = SvcOut;

        #[inline]
        fn modify(&self, input: SvcIn, ctx: Ctx) -> Self::Service {
            (self.0)(input, ctx)
        }
    }

    ModifyServiceFn(f)
}

pub fn modify_service_ref<SvcIn, SvcOut, Request, Ctx>(
    f: impl Fn(SvcIn, &Ctx) -> SvcOut,
) -> impl for<'a> ModifyService<
    &'a Ctx, //
    Request,
    SvcIn,
    Response = SvcOut::Response,
    Error = SvcOut::Error,
    Service = SvcOut,
>
where
    SvcOut: Service<Request>,
{
    #[allow(missing_debug_implementations)]
    struct ModifyServiceRefFn<F>(F);

    impl<'a, F, SvcIn, SvcOut, Request, Ctx> ModifyService<&'a Ctx, Request, SvcIn>
        for ModifyServiceRefFn<F>
    where
        F: Fn(SvcIn, &Ctx) -> SvcOut,
        SvcOut: Service<Request>,
    {
        type Response = SvcOut::Response;
        type Error = SvcOut::Error;
        type Service = SvcOut;

        #[inline]
        fn modify(&self, input: SvcIn, ctx: &'a Ctx) -> Self::Service {
            (self.0)(input, ctx)
        }
    }

    ModifyServiceRefFn(f)
}
