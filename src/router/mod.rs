//! [unstable]
//! The implementation of router used by the framework.

pub mod recognizer;

mod route;
mod router;

pub use self::route::Route;
pub use self::router::{Builder, Mount, Router};
