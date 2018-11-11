use std::fmt;

use crate::internal::scoped_map::ScopeId;
use crate::internal::uri::Uri;

use super::modifier::Modifier;
use super::route::Route;
use super::{AppBuilder, AppResult};

pub(super) struct ScopeData {
    pub(super) id: ScopeId,
    pub(super) parent: ScopeId,
    pub(super) prefix: Uri,
    pub(super) modifiers: Vec<Box<dyn Modifier + Send + Sync + 'static>>,
}

#[cfg_attr(tarpaulin, skip)]
impl fmt::Debug for ScopeData {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.debug_struct("ScopeData")
            .field("id", &self.id)
            .field("parent", &self.parent)
            .field("prefix", &self.prefix)
            .finish()
    }
}

pub(super) struct ScopeBuilder {
    pub(super) id: ScopeId,
    pub(super) parent: ScopeId,
    pub(super) modifiers: Vec<Box<dyn Modifier + Send + Sync + 'static>>,
    pub(super) prefix: Uri,
    pub(super) chain: Vec<ScopeId>,
}

#[cfg_attr(tarpaulin, skip)]
impl fmt::Debug for ScopeBuilder {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.debug_struct("ScopeBuilder")
            .field("parent", &self.parent)
            .field("prefix", &self.prefix)
            .field("chain", &self.chain)
            .finish()
    }
}

/// A proxy object for configuration of a scope.
#[derive(Debug)]
pub struct Scope<'a> {
    builder: &'a mut AppBuilder,
    id: ScopeId,
}

impl<'a> Scope<'a> {
    pub(super) fn new(builder: &'a mut AppBuilder, id: ScopeId) -> Self {
        Self { builder, id }
    }

    /// Adds a route into the current scope.
    pub fn route(&mut self, route: Route) -> &mut Self {
        self.builder.new_route(self.id, route);
        self
    }

    /// Create a new scope mounted to the certain URI.
    #[inline]
    pub fn mount<F, E>(&mut self, prefix: &str, f: F) -> AppResult<&mut Self>
    where
        F: FnOnce(&mut Scope<'_>) -> Result<(), E>,
        E: Into<failure::Error>,
    {
        self.builder.new_scope(self.id, prefix, f)?;
        Ok(self)
    }

    /// Adds a *scope-local* variable into the application.
    pub fn state<T>(&mut self, value: T) -> &mut Self
    where
        T: Send + Sync + 'static,
    {
        self.builder.set_state(value, self.id);
        self
    }

    /// Register a `Modifier` into the current scope.
    pub fn modifier<M>(&mut self, modifier: M) -> &mut Self
    where
        M: Modifier + Send + Sync + 'static,
    {
        self.builder.add_modifier(self.id, modifier);
        self
    }

    #[allow(missing_docs)]
    pub fn done(&mut self) -> AppResult<()> {
        Ok(())
    }
}
