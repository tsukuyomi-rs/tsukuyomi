mod chain;

use tower_service::Service;

pub use self::chain::Chain;

/// A trait representing a *middleware*, which decorates a `Service`.
///
/// This trait has the same signature as `tower_web::middleware::Middleware`,
/// and eventually be replaced to it in the future version.
pub trait Middleware<S> {
    type Request;
    type Response;
    type Error;
    type Service: Service<Request = Self::Request, Response = Self::Response, Error = Self::Error>;

    fn wrap(&self, inner: S) -> Self::Service;
}

#[cfg(feature = "tower-middleware")]
mod tower {
    use tower_web::middleware as tower_middleware;

    impl<M, S> super::Middleware<S> for M
    where
        M: tower_middleware::Middleware<S>,
    {
        type Request = M::Request;
        type Response = M::Response;
        type Error = M::Error;
        type Service = M::Service;

        #[inline]
        fn wrap(&self, inner: S) -> Self::Service {
            <Self as tower_middleware::Middleware<S>>::wrap(self, inner)
        }
    }
}
