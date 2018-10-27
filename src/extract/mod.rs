//! High level API for accessing request data and context information.

#![allow(missing_docs)]

mod extractor;
mod from_input;

pub mod body;
pub mod header;
pub mod param;
pub mod query;
pub mod verb;

pub use self::extractor::{extract, EitherOf, Extractor, Fallible, Optional, Preflight};
pub use self::from_input::{Directly, Extension, FromInput, Local, State};
