#![allow(dead_code)]

pub trait BuilderExt {
    fn if_some<T>(self, v: Option<T>, f: impl FnOnce(Self, T) -> Self) -> Self
    where
        Self: Sized,
    {
        match v {
            Some(v) => f(self, v),
            None => self,
        }
    }
}

impl<T> BuilderExt for T {}
