use error::Error;
use http::Request;
use hyperx::header::Header;

/// A set of extensions for `Request<T>`.
pub trait RequestExt: sealed::Sealed {
    /// Get the value of header field and parse it to a certain type.
    fn header<H>(&self) -> Result<Option<H>, Error>
    where
        H: Header;
}

impl<T> RequestExt for Request<T> {
    fn header<H>(&self) -> Result<Option<H>, Error>
    where
        H: Header,
    {
        match self.headers().get(H::header_name()) {
            Some(h) => H::parse_header(&h.as_bytes().into())
                .map_err(Error::bad_request)
                .map(Some),
            None => Ok(None),
        }
    }
}

mod sealed {
    use http::Request;

    pub trait Sealed {}

    impl<T> Sealed for Request<T> {}
}
