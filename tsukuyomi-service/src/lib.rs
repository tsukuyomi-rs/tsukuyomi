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
pub trait MakeService<Target, Request> {
    type Response;
    type Error;
    type Service: Service<Request, Response = Self::Response, Error = Self::Error>;
    type MakeError;
    type Future: Future<Item = Self::Service, Error = Self::MakeError>;

    fn make_service(&self, target: Target) -> Self::Future;
}

pub trait MakeServiceRef<Target, Request> {
    type Response;
    type Error;
    type Service: Service<Request, Response = Self::Response, Error = Self::Error>;
    type MakeError;
    type Future: Future<Item = Self::Service, Error = Self::MakeError>;

    fn make_service_ref(&self, target: &Target) -> Self::Future;
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
    fn make_service_ref(&self, target: &T) -> Self::Future {
        MakeService::make_service(self, target)
    }
}
