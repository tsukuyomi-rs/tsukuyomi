//! Extractors for parsing parameters in HTTP path.

#![allow(missing_docs)]

use failure::format_err;
use failure::Fail;
use std::marker::PhantomData;
use std::str::FromStr;

use crate::error::Failure;
use crate::extractor::{Extract, Extractor};
use crate::input::Input;

#[derive(Debug)]
pub struct Pos<T> {
    pos: usize,
    _marker: PhantomData<fn() -> T>,
}

impl<T> Pos<T>
where
    T: FromStr + 'static,
    T::Err: Fail,
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
    T::Err: Fail,
{
    type Output = T;
    type Error = Failure;
    type Future = super::Placeholder<T, Failure>;

    fn extract(&self, input: &mut Input<'_>) -> Result<Extract<Self>, Self::Error> {
        let params = input.params();
        let s = params.get(self.pos).ok_or_else(|| {
            Failure::internal_server_error(format_err!("the cursor is out of range"))
        })?;
        s.parse().map(Extract::Ready).map_err(Failure::bad_request)
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
    T::Err: Fail,
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
    T::Err: Fail,
{
    type Output = T;
    type Error = Failure;
    type Future = super::Placeholder<Self::Output, Self::Error>;

    fn extract(&self, input: &mut Input<'_>) -> Result<Extract<Self>, Self::Error> {
        let params = input.params();
        let s = params.name(&self.name).ok_or_else(|| {
            Failure::internal_server_error(format_err!("the cursor is out of range"))
        })?;
        s.parse().map(Extract::Ready).map_err(Failure::bad_request)
    }
}

#[derive(Debug)]
pub struct Wildcard<T>(PhantomData<fn() -> T>);

impl<T> Default for Wildcard<T>
where
    T: FromStr + 'static,
    T::Err: Fail,
{
    fn default() -> Self {
        Self::new()
    }
}

impl<T> Wildcard<T>
where
    T: FromStr + 'static,
    T::Err: Fail,
{
    pub fn new() -> Self {
        Wildcard(PhantomData)
    }
}

impl<T> Extractor for Wildcard<T>
where
    T: FromStr + 'static,
    T::Err: Fail,
{
    type Output = T;
    type Error = Failure;
    type Future = super::Placeholder<Self::Output, Self::Error>;

    fn extract(&self, input: &mut Input<'_>) -> Result<Extract<Self>, Self::Error> {
        let params = input.params();
        let s = params.get_wildcard().ok_or_else(|| {
            Failure::internal_server_error(format_err!("the wildcard parameter is not set"))
        })?;
        s.parse().map(Extract::Ready).map_err(Failure::bad_request)
    }
}
