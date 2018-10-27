//! Extractors for accessing HTTP header fields.

use mime::Mime;

use crate::error::Failure;
use crate::input::Input;

use super::extractor::{Extractor, Preflight};

/// The instance of `FromInput` which extracts the header field `Content-type`.
#[derive(Debug)]
pub struct ContentType {
    _priv: (),
}

impl Extractor for ContentType {
    type Out = Mime;
    type Error = Failure;
    type Ctx = ();

    fn preflight(&self, input: &mut Input<'_>) -> Result<Preflight<Self>, Self::Error> {
        match crate::input::header::content_type(input)? {
            Some(mime) => Ok(Preflight::Completed(mime.clone())),
            None => Err(Failure::bad_request(failure::format_err!(
                "missing Content-type"
            ))),
        }
    }
}
