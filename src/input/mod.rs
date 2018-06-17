//! Components for parsing incoming HTTP requests.

pub mod body;

mod cookie;
mod input;

#[doc(inline)]
pub use self::body::RequestBody;

#[doc(inline)]
pub use self::cookie::Cookies;

#[doc(inline)]
pub use self::input::{Input, Params};

pub(crate) use self::input::InputParts;
