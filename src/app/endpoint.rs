use http::Method;

use handler::Handler;

use super::uri::Uri;

/// A type representing an endpoint.
///
/// The value of this type contains a `Handler` to handle the accepted HTTP request,
/// and some information for constructing a `Router`.
#[derive(Debug)]
pub struct Endpoint {
    uri: Uri,
    method: Method,
    handler: Handler,
}

impl Endpoint {
    pub(crate) fn new(uri: Uri, method: Method, handler: Handler) -> Endpoint {
        Endpoint {
            uri: uri,
            method: method,
            handler: handler,
        }
    }

    /// Returns the full HTTP path of this endpoint.
    pub fn uri(&self) -> &Uri {
        &self.uri
    }

    /// Returns the reference to `Method` which this route allows.
    pub fn method(&self) -> &Method {
        &self.method
    }

    /// Returns the reference to `Handler` associated with this endpoint.
    pub fn handler(&self) -> &Handler {
        &self.handler
    }
}
