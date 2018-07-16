use http::Method;

use handler::Handler;

use super::uri::Uri;
use super::ScopeId;

/// A type representing an endpoint.
///
/// The value of this type contains a `Handler` to handle the accepted HTTP request,
/// and some information for constructing a `Router`.
#[derive(Debug)]
pub struct Endpoint {
    pub(super) uri: Uri,
    pub(super) method: Method,
    pub(super) scope_id: ScopeId,
    pub(super) handler: Handler,
}

impl Endpoint {
    /// Returns the full HTTP path of this endpoint.
    pub fn uri(&self) -> &Uri {
        &self.uri
    }

    /// Returns the reference to `Method` which this route allows.
    pub fn method(&self) -> &Method {
        &self.method
    }

    pub(crate) fn scope_id(&self) -> ScopeId {
        self.scope_id
    }

    /// Returns the reference to `Handler` associated with this endpoint.
    pub fn handler(&self) -> &Handler {
        &self.handler
    }
}
