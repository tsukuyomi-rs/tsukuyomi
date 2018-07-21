use http::header::HeaderMap;
use http::StatusCode;

/// [unstable]
/// A trait representing additional information for constructing an HTTP response.
pub trait HttpResponse {
    /// Returns an HTTP status code associated with the value of this type.
    fn status_code(&self) -> StatusCode {
        StatusCode::OK
    }

    /// Appends some entries into the header map of an HTTP response.
    #[allow(unused_variables)]
    fn append_headers(&self, headers: &mut HeaderMap) {}
}
