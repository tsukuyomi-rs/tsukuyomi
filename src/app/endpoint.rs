use http::Method;
use std::fmt;

use handler::Handler;
use modifier::Modifier;

use super::uri::Uri;
use super::{ModifierId, ScopeId};

/// A type representing an endpoint.
///
/// The value of this type contains a `Handler` to handle the accepted HTTP request,
/// and some information for constructing a `Router`.
pub struct Endpoint {
    pub(super) uri: Uri,
    pub(super) method: Method,
    pub(super) scope_id: ScopeId,
    pub(super) modifiers: Vec<Box<dyn Modifier + Send + Sync + 'static>>,
    pub(super) handler: Box<dyn Handler + Send + Sync + 'static>,
    pub(super) modifier_ids: Vec<ModifierId>,
}

#[cfg_attr(tarpaulin, skip)]
impl fmt::Debug for Endpoint {
    fn fmt(&self, f: &mut fmt::Formatter) -> fmt::Result {
        f.debug_struct("Endpoint")
            .field("uri", &self.uri)
            .field("method", &self.method)
            .field("scope_id", &self.scope_id)
            .field("modifier_ids", &self.modifier_ids)
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
}
