//! Extractors for parsing parameters in HTTP path.

#![allow(missing_docs)]

use std::fmt;
use std::marker::PhantomData;
use std::str::FromStr;

use crate::error::Error;
use crate::extractor::{Extract, Extractor};
use crate::input::Input;

pub fn pos<T>(pos: usize) -> Pos<T>
where
    T: FromStr + 'static,
    T::Err: fmt::Debug + fmt::Display + Send + 'static,
{
    Pos::new(pos)
}

pub fn named<T>(name: impl Into<String>) -> Named<T>
where
    T: FromStr + 'static,
    T::Err: fmt::Debug + fmt::Display + Send + 'static,
{
    Named::new(name)
}

pub fn wildcard<T>() -> Wildcard<T>
where
    T: FromStr + 'static,
    T::Err: fmt::Debug + fmt::Display + Send + 'static,
{
    Wildcard::new()
}

#[derive(Debug)]
pub struct Pos<T> {
    pos: usize,
    _marker: PhantomData<fn() -> T>,
}

impl<T> Pos<T>
where
    T: FromStr + 'static,
    T::Err: fmt::Debug + fmt::Display + Send + 'static,
{
    pub fn new(pos: usize) -> Self {
        Self {
            pos,
            _marker: PhantomData,
        }
    }
}

impl<T> Extractor for Pos<T>
where
    T: FromStr + 'static,
    T::Err: fmt::Debug + fmt::Display + Send + 'static,
{
    type Output = (T,);
    type Error = Error;
    type Future = super::Placeholder<Self::Output, Self::Error>;

    fn extract(&self, input: &mut Input<'_>) -> Result<Extract<Self>, Self::Error> {
        let params = input.params();
        let s = params
            .get(self.pos)
            .ok_or_else(|| crate::error::internal_server_error("the cursor is out of range"))?;
        s.parse()
            .map(|out| Extract::Ready((out,)))
            .map_err(crate::error::bad_request)
    }
}

#[derive(Debug)]
pub struct Named<T> {
    name: String,
    _marker: PhantomData<fn() -> T>,
}

impl<T> Named<T>
where
    T: FromStr + 'static,
    T::Err: fmt::Debug + fmt::Display + Send + 'static,
{
    pub fn new(name: impl Into<String>) -> Self {
        Self {
            name: name.into(),
            _marker: PhantomData,
        }
    }
}

impl<T> Extractor for Named<T>
where
    T: FromStr + 'static,
    T::Err: fmt::Debug + fmt::Display + Send + 'static,
{
    type Output = (T,);
    type Error = Error;
    type Future = super::Placeholder<Self::Output, Self::Error>;

    fn extract(&self, input: &mut Input<'_>) -> Result<Extract<Self>, Self::Error> {
        let params = input.params();
        let s = params
            .name(&self.name)
            .ok_or_else(|| crate::error::internal_server_error("the cursor is out of range"))?;
        s.parse()
            .map(|out| Extract::Ready((out,)))
            .map_err(crate::error::bad_request)
    }
}

#[derive(Debug)]
pub struct Wildcard<T>(PhantomData<fn() -> T>);

impl<T> Default for Wildcard<T>
where
    T: FromStr + 'static,
    T::Err: fmt::Debug + fmt::Display + Send + 'static,
{
    fn default() -> Self {
        Self::new()
    }
}

impl<T> Wildcard<T>
where
    T: FromStr + 'static,
    T::Err: fmt::Debug + fmt::Display + Send + 'static,
{
    pub fn new() -> Self {
        Wildcard(PhantomData)
    }
}

impl<T> Extractor for Wildcard<T>
where
    T: FromStr + 'static,
    T::Err: fmt::Debug + fmt::Display + Send + 'static,
{
    type Output = (T,);
    type Error = Error;
    type Future = super::Placeholder<Self::Output, Self::Error>;

    fn extract(&self, input: &mut Input<'_>) -> Result<Extract<Self>, Self::Error> {
        let params = input.params();
        let s = params.get_wildcard().ok_or_else(|| {
            crate::error::internal_server_error("the wildcard parameter is not set")
        })?;
        s.parse()
            .map(|out| Extract::Ready((out,)))
            .map_err(crate::error::bad_request)
    }
}
