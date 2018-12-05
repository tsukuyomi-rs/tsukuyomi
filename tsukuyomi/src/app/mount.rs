//! The definition of `Scope` and its implementors.

use {
    super::{
        builder::{Context, Scope},
        fallback::{BoxedFallback, Fallback},
        Uri,
    },
    crate::common::{Chain, TryFrom},
};

/// A function that creates a `Mount` with the empty scope items.
pub fn mount<T>(prefix: T) -> super::Result<Mount<(), ()>>
where
    Uri: TryFrom<T>,
{
    let prefix = Uri::try_from(prefix)?;
    Ok(Mount {
        scope: (),
        modifier: (),
        fallback: None,
        prefix,
    })
}

/// An instance of `Scope` that represents a sub-scope with a specific prefix.
#[allow(missing_debug_implementations)]
pub struct Mount<S = (), M = ()> {
    scope: S,
    modifier: M,
    fallback: Option<BoxedFallback>,
    prefix: Uri,
}

#[cfg_attr(feature = "cargo-clippy", allow(use_self))]
impl<S, M> Mount<S, M> {
    /// Merges the specified `Scope` into the inner scope, *without* creating a new subscope.
    pub fn with<S2>(self, next_scope: S2) -> Mount<Chain<S, S2>, M> {
        Mount {
            scope: Chain::new(self.scope, next_scope),
            modifier: self.modifier,
            fallback: self.fallback,
            prefix: self.prefix,
        }
    }

    pub fn modifier<M2>(self, modifier: M2) -> Mount<S, Chain<M, M2>> {
        Mount {
            scope: self.scope,
            modifier: Chain::new(self.modifier, modifier),
            fallback: self.fallback,
            prefix: self.prefix,
        }
    }

    pub fn fallback<F>(self, fallback: F) -> Self
    where
        F: Fallback,
    {
        Self {
            fallback: Some(fallback.into()),
            ..self
        }
    }
}

impl<S, M1, M2> Scope<M1> for Mount<S, M2>
where
    M1: Clone,
    S: Scope<Chain<M1, M2>>,
{
    type Error = super::Error;

    fn configure(self, cx: &mut Context<'_, M1>) -> std::result::Result<(), Self::Error> {
        cx.add_scope(self.prefix, self.modifier, self.fallback, self.scope)
    }
}
