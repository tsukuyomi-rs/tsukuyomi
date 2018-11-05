use std::fmt;

use crate::modifier::Modifier;
use crate::recognizer::uri::Uri;

use super::route::Route;
use super::{AppBuilder, Global};

#[derive(Debug, Copy, Clone, PartialEq, Eq, Hash)]
pub(crate) enum ScopeId {
    Global,
    Local(usize),
}

impl ScopeId {
    pub(super) fn local_id(self) -> Option<usize> {
        match self {
            ScopeId::Global => None,
            ScopeId::Local(id) => Some(id),
        }
    }
}

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

    /// Returns a proxy object for modifying the global-level configuration.
    pub fn global(&mut self) -> Global<'_> {
        Global {
            builder: &mut *self.builder,
        }
    }

    /// Adds a route into the current scope.
    pub fn route(&mut self, route: Route) -> &mut Self {
        self.builder.new_route(self.id, route);
        self
    }

    /// Create a new scope mounted to the certain URI.
    #[inline(always)]
    pub fn mount<F>(&mut self, prefix: &str, f: F) -> &mut Self
    where
        F: FnOnce(&mut Scope<'_>),
    {
        self.builder.new_scope(self.id, prefix, f);
        self
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

    /// Report an error to the builder context.
    ///
    /// After calling this method, all operations in the builder are invalidated and
    /// `AppBuilder::finish()` always returns an error.
    pub fn mark_error<E>(&mut self, err: E) -> &mut Self
    where
        E: Into<failure::Error>,
    {
        self.builder.mark_error(err);
        self
    }
}
