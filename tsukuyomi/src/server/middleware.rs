use tower_service::Service;

#[cfg(feature = "tower-middleware")]
pub use self::tower::Compat;

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

    fn chain<O>(self, outer: O) -> Chain<Self, O>
    where
        Self: Sized,
        O: Middleware<Self::Service>,
    {
        Chain { inner: self, outer }
    }
}

#[cfg(feature = "tower-middleware")]
mod tower {
    use tower_web::middleware as tower_middleware;

    #[derive(Debug, Clone)]
    pub struct Compat<M>(pub(crate) M);

    impl<M, S> super::Middleware<S> for Compat<M>
    where
        M: tower_middleware::Middleware<S>,
    {
        type Request = M::Request;
        type Response = M::Response;
        type Error = M::Error;
        type Service = M::Service;

        #[inline]
        fn wrap(&self, inner: S) -> Self::Service {
            self.0.wrap(inner)
        }
    }
}

#[derive(Debug, Default)]
pub struct Identity(());

impl<S: Service> Middleware<S> for Identity {
    type Request = S::Request;
    type Response = S::Response;
    type Error = S::Error;
    type Service = S;

    #[inline]
    fn wrap(&self, service: S) -> Self::Service {
        service
    }
}

#[derive(Debug)]
pub struct Chain<I, O> {
    inner: I,
    outer: O,
}

impl<I, O, S> Middleware<S> for Chain<I, O>
where
    S: Service,
    I: Middleware<S>,
    O: Middleware<I::Service>,
{
    type Request = O::Request;
    type Response = O::Response;
    type Error = O::Error;
    type Service = O::Service;

    #[inline]
    fn wrap(&self, service: S) -> Self::Service {
        self.outer.wrap(self.inner.wrap(service))
    }
}
