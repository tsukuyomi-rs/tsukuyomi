use http::Method;
use std::fmt;

use filter::{Filter, Filtering};
use handler::{Handle, Handler};
use input::Input;

use super::uri::Uri;
use super::ScopeId;

/// A type representing an endpoint.
///
/// The value of this type contains a `Handler` to handle the accepted HTTP request,
/// and some information for constructing a `Router`.
pub struct Endpoint {
    pub(super) uri: Uri,
    pub(super) method: Method,
    pub(super) scope_id: ScopeId,
    pub(super) filters: Vec<Box<dyn Filter + Send + Sync + 'static>>,
    pub(super) handler: Box<dyn Handler + Send + Sync + 'static>,
}

#[cfg_attr(tarpaulin, skip)]
impl fmt::Debug for Endpoint {
    fn fmt(&self, f: &mut fmt::Formatter) -> fmt::Result {
        f.debug_struct("Endpoint")
            .field("uri", &self.uri)
            .field("method", &self.method)
            .field("scope_id", &self.scope_id)
            .finish()
    }
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

    pub(crate) fn apply_filter(&self, input: &mut Input, pos: usize) -> Option<Filtering> {
        self.filters.get(pos).map(|filter| filter.apply(input))
    }

    pub(crate) fn apply_handler(&self, input: &mut Input) -> Handle {
        self.handler.handle(input)
    }
}
