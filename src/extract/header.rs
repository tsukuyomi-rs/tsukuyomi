//! Extractors for accessing HTTP header fields.

use mime::Mime;
use std::ops::Deref;

use crate::error::Failure;
use crate::input::Input;

use super::{FromInput, Preflight};

/// The instance of `FromInput` which extracts the header field `Content-type`.
#[derive(Debug)]
pub struct ContentType(pub Mime);

impl AsRef<Mime> for ContentType {
    #[inline]
    fn as_ref(&self) -> &Mime {
        &self.0
    }
}

impl Deref for ContentType {
    type Target = Mime;

    #[inline]
    fn deref(&self) -> &Self::Target {
        &self.0
    }
}

impl FromInput for ContentType {
    type Error = Failure;
    type Ctx = ();

    fn preflight(input: &mut Input<'_>) -> Result<Preflight<Self>, Self::Error> {
        match crate::input::header::content_type(input)? {
            Some(mime) => Ok(Preflight::Completed(ContentType(mime.clone()))),
            None => Err(Failure::bad_request(failure::format_err!(
                "missing Content-type"
            ))),
        }
    }
}
