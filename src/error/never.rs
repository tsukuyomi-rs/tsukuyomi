use failure::Fail;
use std::fmt;

use super::HttpError;

/// A helper type emulating the standard never_type (`!`).
#[derive(Debug)]
pub enum Never {}

impl fmt::Display for Never {
    fn fmt(&self, _: &mut fmt::Formatter<'_>) -> fmt::Result {
        unreachable!()
    }
}

impl Fail for Never {}

impl HttpError for Never {}
