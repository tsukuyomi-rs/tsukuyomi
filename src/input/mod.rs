//! Components for parsing incoming HTTP requests.

pub mod body;

mod request;

#[doc(inline)]
pub use self::body::RequestBody;

#[doc(inline)]
pub use self::request::RequestExt;
