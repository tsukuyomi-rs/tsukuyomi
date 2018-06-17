//! [unstable]
//! The implementation of router used by the framework.

pub mod recognizer;
pub mod uri;

mod endpoint;
mod router;

pub use self::endpoint::Endpoint;
pub use self::router::{Builder, Mount, Route, Router};
#[doc(inline)]
pub use self::uri::Uri;

pub(crate) use self::endpoint::Handle;
