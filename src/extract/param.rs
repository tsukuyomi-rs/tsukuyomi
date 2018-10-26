//! Extractors for parsing parameters in HTTP path.

use failure::format_err;
use failure::Fail;
use std::ops::Deref;
use std::str::FromStr;

use crate::error::Failure;
use crate::input::Input;

use super::{FromInput, Preflight};

/// The instance of `FromInput` which extracts a parameter in HTTP path.
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

impl<T> FromInput for Param<T>
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
}

/// The instance of `FromInput` which extracts the wildcard parameter in HTTP path.
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

impl<T> FromInput for Wildcard<T>
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
}
