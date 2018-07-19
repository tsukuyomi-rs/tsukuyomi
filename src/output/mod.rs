//! Components for constructing HTTP responses.

pub mod upgrade;

mod body;
mod responder;
mod response;

// re-exports
pub(crate) use self::body::Receive;
pub use self::body::{Data, ResponseBody};
pub use self::responder::Responder;
pub use self::response::{HttpResponse, ResponseExt};

// =====

use self::upgrade::BoxedUpgradeHandler;
use http::Response;
use std::fmt;
use std::ops::{Deref, DerefMut};

/// The type representing outputs returned from handlers.
///
/// The values of this type are constructed indirectly by `Responder`, or by converting from the
/// value of `Response<T>`.
pub struct Output {
    pub(crate) response: Response<ResponseBody>,
    pub(crate) upgrade: Option<BoxedUpgradeHandler>,
}

impl Output {
    #[allow(missing_docs)]
    pub fn is_upgraded(&self) -> bool {
        self.upgrade.is_some()
    }
}

impl Deref for Output {
    type Target = Response<ResponseBody>;

    fn deref(&self) -> &Self::Target {
        &self.response
    }
}

impl DerefMut for Output {
    fn deref_mut(&mut self) -> &mut Self::Target {
        &mut self.response
    }
}

#[cfg_attr(tarpaulin, skip)]
impl fmt::Debug for Output {
    fn fmt(&self, f: &mut fmt::Formatter) -> fmt::Result {
        f.debug_struct("Output").field("response", &self.response).finish()
    }
}

impl<T> From<Response<T>> for Output
where
    T: Into<ResponseBody>,
{
    fn from(response: Response<T>) -> Self {
        Output {
            response: response.map(|bd| bd.into()),
            upgrade: None,
        }
    }
}

impl Output {
    pub(crate) fn deconstruct(self) -> (Response<ResponseBody>, Option<BoxedUpgradeHandler>) {
        (self.response, self.upgrade)
    }
}
