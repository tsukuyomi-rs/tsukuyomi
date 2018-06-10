//! The implementation of router used by the framework.

pub mod recognizer;

mod handler;
mod route;
mod router;

pub use self::handler::Handler;
pub use self::route::Route;
pub use self::router::{Builder, Router, RouterState};
