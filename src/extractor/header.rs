//! Extractors for accessing HTTP header fields.

use mime::Mime;

use crate::error::Failure;
use crate::extractor::{Extract, Extractor};
use crate::input::Input;

/// The instance of `FromInput` which extracts the header field `Content-type`.
#[derive(Debug)]
pub struct ContentType {
    _priv: (),
}

impl Extractor for ContentType {
    type Output = Mime;
    type Error = Failure;
    type Future = super::Placeholder<Mime, Failure>;

    fn extract(&self, input: &mut Input<'_>) -> Result<Extract<Self>, Self::Error> {
        match crate::input::header::content_type(input)? {
            Some(mime) => Ok(Extract::Ready(mime.clone())),
            None => Err(Failure::bad_request(failure::format_err!(
                "missing Content-type"
            ))),
        }
    }
}
