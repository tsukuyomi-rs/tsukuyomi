//! Components for constructing HTTP responses.

mod body;
mod responder;
mod response;

// re-exports
pub use self::body::ResponseBody;
pub(crate) use self::body::ResponseBodyKind;
pub use self::responder::Responder;
pub use self::response::HttpResponse;

/// The type representing outputs returned from handlers.
pub type Output = ::http::Response<ResponseBody>;
