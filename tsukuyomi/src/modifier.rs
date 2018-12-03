//! The definition of `Modifier`.
//!
//! The purpose of this trait is to insert some processes before and after
//! applying `Handler` in a certain scope.

use crate::{common::Chain, handler::Handler};

/// A trait representing a `Modifier`.
pub trait Modifier<H: Handler> {
    type Out: Handler;

    fn modify(&self, inner: H) -> Self::Out;

    fn chain<M>(self, next: M) -> Chain<Self, M>
    where
        Self: Sized,
    {
        Chain::new(self, next)
    }
}

impl<'a, M, H> Modifier<H> for &'a M
where
    M: Modifier<H> + 'a,
    H: Handler,
{
    type Out = M::Out;

    #[inline]
    fn modify(&self, inner: H) -> Self::Out {
        (*self).modify(inner)
    }
}

impl<M, H> Modifier<H> for std::rc::Rc<M>
where
    M: Modifier<H>,
    H: Handler,
{
    type Out = M::Out;

    #[inline]
    fn modify(&self, inner: H) -> Self::Out {
        (**self).modify(inner)
    }
}

impl<H> Modifier<H> for ()
where
    H: Handler,
{
    type Out = H;

    #[inline]
    fn modify(&self, inner: H) -> Self::Out {
        inner
    }
}

impl<M1, M2, H> Modifier<H> for Chain<M1, M2>
where
    M1: Modifier<M2::Out>,
    M2: Modifier<H>,
    H: Handler,
{
    type Out = M1::Out;

    #[inline]
    fn modify(&self, inner: H) -> Self::Out {
        self.left.modify(self.right.modify(inner))
    }
}
