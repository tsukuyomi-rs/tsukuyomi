use http::Response;
use hyper::Body;

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
    pub(crate) fn into_response(self) -> Response<ResponseBody> {
        self.0
    }
}

pub type ResponseBody = Body;
