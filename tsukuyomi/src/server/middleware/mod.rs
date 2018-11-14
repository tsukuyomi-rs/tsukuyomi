mod chain;

use tower_service::Service;

pub use self::chain::Chain;

pub trait Middleware<S> {
    type Request;
    type Response;
    type Error;
    type Service: Service<Request = Self::Request, Response = Self::Response, Error = Self::Error>;

    fn wrap(&self, inner: S) -> Self::Service;
}
