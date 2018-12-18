#![doc(html_root_url = "https://docs.rs/tsukuyomi-service/0.1.0-dev")]
#![deny(
    missing_debug_implementations,
    nonstandard_style,
    rust_2018_idioms,
    rust_2018_compatibility,
    unused
)]
#![forbid(clippy::unimplemented)]

use futures::{Future, Poll};

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

    fn poll_ready(&mut self) -> Poll<(), Self::MakeError>;

    fn make_service(&self, target: Target) -> Self::Future;
}
