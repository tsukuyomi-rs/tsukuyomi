//! Components for accessing extracted parameters from HTTP path.

use bytes::Bytes;
use failure::{format_err, Fail};
use std::ops::{Deref, DerefMut, Index};
use std::str::FromStr;

use super::from_input::{FromInput, FromInputImpl, Preflight};
use super::Input;
use crate::error::Failure;
use crate::recognizer::captures::{CaptureNames, Captures};

/// A proxy object for accessing extracted parameters.
#[derive(Debug)]
pub struct Params<'input> {
    path: &'input str,
    names: Option<&'input CaptureNames>,
    captures: Option<&'input Captures>,
}

impl<'input> Params<'input> {
    pub(crate) fn new(
        path: &'input str,
        names: Option<&'input CaptureNames>,
        captures: Option<&'input Captures>,
    ) -> Params<'input> {
        debug_assert_eq!(names.is_some(), captures.is_some());
        Params {
            path,
            names,
            captures,
        }
    }

    /// Returns `true` if the extracted paramater exists.
    pub fn is_empty(&self) -> bool {
        self.captures.map_or(true, |caps| {
            caps.params.is_empty() && caps.wildcard.is_none()
        })
    }

    /// Returns the value of `i`-th parameter, if exists.
    pub fn get(&self, i: usize) -> Option<&str> {
        let &(s, e) = self.captures?.params.get(i)?;
        self.path.get(s..e)
    }

    /// Returns the value of wildcard parameter, if exists.
    pub fn get_wildcard(&self) -> Option<&str> {
        let (s, e) = self.captures?.wildcard?;
        self.path.get(s..e)
    }

    /// Returns the value of parameter whose name is equal to `name`, if exists.
    pub fn name(&self, name: &str) -> Option<&str> {
        match name {
            "*" => self.get_wildcard(),
            name => self.get(self.names?.params.get_full(name)?.0),
        }
    }
}

impl<'input> Index<usize> for Params<'input> {
    type Output = str;

    fn index(&self, i: usize) -> &Self::Output {
        self.get(i).expect("Out of range")
    }
}

impl<'input, 'a> Index<&'a str> for Params<'input> {
    type Output = str;

    fn index(&self, name: &'a str) -> &Self::Output {
        self.name(name).expect("Out of range")
    }
}

/// The instance of `FromInput` which extract a parameter.
#[derive(Debug)]
pub struct Param<T>(pub T);

impl<T> Param<T> {
    #[allow(missing_docs)]
    #[cfg_attr(tarpaulin, skip)]
    pub fn into_inner(self) -> T {
        self.0
    }
}

impl<T> Deref for Param<T> {
    type Target = T;

    #[cfg_attr(tarpaulin, skip)]
    fn deref(&self) -> &Self::Target {
        &self.0
    }
}

impl<T> DerefMut for Param<T> {
    #[cfg_attr(tarpaulin, skip)]
    fn deref_mut(&mut self) -> &mut Self::Target {
        &mut self.0
    }
}

impl<T> FromInput for Param<T>
where
    T: FromStr + 'static,
    T::Err: Fail,
{
}
impl<T> FromInputImpl for Param<T>
where
    T: FromStr + 'static,
    T::Err: Fail,
{
    type Error = Failure;
    type Ctx = ();

    fn preflight(input: &mut Input<'_>) -> Result<Preflight<Self>, Self::Error> {
        let i = *input.cursor();
        *input.cursor() += 1;
        let params = input.params();
        let s = params.get(i).ok_or_else(|| {
            Failure::internal_server_error(format_err!("the cursor is out of range"))
        })?;
        s.parse()
            .map(|val| Preflight::Completed(Param(val)))
            .map_err(Failure::bad_request)
    }

    fn extract(_: &Bytes, _: &mut Input<'_>, _: ()) -> Result<Self, Self::Error> {
        unreachable!()
    }
}

/// The instance of `FromInput` which extract the wildcard parameter.
#[derive(Debug)]
pub struct Wildcard<T>(pub T);

impl<T> Wildcard<T> {
    #[allow(missing_docs)]
    #[cfg_attr(tarpaulin, skip)]
    pub fn into_inner(self) -> T {
        self.0
    }
}

impl<T> Deref for Wildcard<T> {
    type Target = T;

    #[cfg_attr(tarpaulin, skip)]
    fn deref(&self) -> &Self::Target {
        &self.0
    }
}

impl<T> DerefMut for Wildcard<T> {
    #[cfg_attr(tarpaulin, skip)]
    fn deref_mut(&mut self) -> &mut Self::Target {
        &mut self.0
    }
}

impl<T> FromInput for Wildcard<T>
where
    T: FromStr + 'static,
    T::Err: Fail,
{
}
impl<T> FromInputImpl for Wildcard<T>
where
    T: FromStr + 'static,
    T::Err: Fail,
{
    type Error = Failure;
    type Ctx = ();

    fn preflight(input: &mut Input<'_>) -> Result<Preflight<Self>, Self::Error> {
        let params = input.params();
        let s = params.get_wildcard().ok_or_else(|| {
            Failure::internal_server_error(format_err!("the wildcard parameter is not set"))
        })?;
        s.parse()
            .map(|val| Preflight::Completed(Wildcard(val)))
            .map_err(Failure::bad_request)
    }

    fn extract(_: &Bytes, _: &mut Input<'_>, _: ()) -> Result<Self, Self::Error> {
        unreachable!()
    }
}
