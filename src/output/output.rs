use http::Response;
use hyper::body::Body;

use super::body::ResponseBody;

#[derive(Debug)]
pub struct Output(Response<ResponseBody>);

impl<T> From<Response<T>> for Output
where
    T: Into<ResponseBody>,
{
    fn from(response: Response<T>) -> Self {
        Output(response.map(Into::into))
    }
}

impl Output {
    pub(crate) fn deconstruct(self) -> Response<Body> {
        self.0.map(ResponseBody::into_hyp)
    }
}
