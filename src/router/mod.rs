//! [unstable]
//! The implementation of router used by the framework.

pub mod recognizer;

mod endpoint;
mod router;
mod uri;

pub use self::endpoint::Endpoint;
pub use self::router::{Builder, Mount, Route, Router};
pub use self::uri::Uri;
