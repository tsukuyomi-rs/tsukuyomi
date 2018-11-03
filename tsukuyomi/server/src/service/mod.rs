#![allow(missing_docs)]

mod builder;
pub mod http;
pub mod middleware;

pub use self::builder::NewServiceExt;

#[doc(no_inline)]
pub use tower_service::{NewService, Service};
