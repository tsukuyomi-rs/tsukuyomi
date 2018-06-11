//! [unstable]
//! The implementation of router used by the framework.

pub mod recognizer;

mod handler;
mod route;
mod router;

pub use self::handler::Handler;
pub use self::route::{Route, Verb};
pub use self::router::{Builder, Mount, Router};
