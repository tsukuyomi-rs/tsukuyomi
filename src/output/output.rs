use http::Response;
use hyper::body::Body;
use std::fmt;

use upgrade::BoxedUpgradeHandler;

use super::body::ResponseBody;

/// The type representing outputs returned from handlers.
///
/// The values of this type are constructed indirectly by `Responder`, or by converting from the
/// value of `Response<T>`.
pub struct Output {
    pub(crate) response: Response<Body>,
    pub(crate) upgrade: Option<BoxedUpgradeHandler>,
}

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
            response: response.map(|bd| bd.into().into_hyp()),
            upgrade: None,
        }
    }
}

impl Output {
    pub(crate) fn deconstruct(self) -> (Response<Body>, Option<BoxedUpgradeHandler>) {
        (self.response, self.upgrade)
    }
}
