//! Components for constructing HTTP responses.

pub mod upgrade;

mod body;
mod output;
mod responder;
mod response;

pub use self::body::{Data, ResponseBody};
pub use self::output::Output;
pub use self::responder::Responder;
pub use self::response::{HttpResponse, ResponseExt};

pub(crate) use self::body::Receive;
