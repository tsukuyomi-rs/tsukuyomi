pub mod handler;

use failure;
use http::{header, Response, StatusCode};
use std::error::Error as StdError;

use output::ResponseBody;

pub type CritError = Box<StdError + Send + Sync + 'static>;

#[derive(Debug)]
pub struct Error {
    kind: ErrorKind,
}

#[derive(Debug)]
enum ErrorKind {
    Failed(failure::Error, StatusCode),
    Crit(CritError),
}

impl<E> From<E> for Error
where
    E: Into<failure::Error>,
{
    fn from(err: E) -> Error {
        Error::new(err, StatusCode::INTERNAL_SERVER_ERROR)
    }
}

impl Error {
    /// Constructs an HTTP error from components.
    pub fn new<E>(err: E, status: StatusCode) -> Error
    where
        E: Into<failure::Error>,
    {
        Error {
            kind: ErrorKind::Failed(err.into(), status),
        }
    }

    /// Constructs a *critical* error from a value.
    ///
    /// The word *critical* means that the error does not converted to an HTTP response and will be
    /// passed directly to the lower-level HTTP service.
    pub fn critical<E>(err: E) -> Error
    where
        E: StdError + Send + Sync + 'static,
    {
        Error {
            kind: ErrorKind::Crit(Box::new(err)),
        }
    }

    /// Returns `true` if this error is a *critical* error.
    pub fn is_critical(&self) -> bool {
        match self.kind {
            ErrorKind::Crit(..) => true,
            _ => false,
        }
    }

    /// Constructs an HTTP response from this error value.
    ///
    /// If this error is a critical error, it does not be converted to an HTTP response and
    /// immediately returns an `Err`.
    pub fn into_response(self) -> Result<Response<ResponseBody>, CritError> {
        match self.kind {
            ErrorKind::Failed(e, status) => Response::builder()
                .status(status)
                .header(header::CONNECTION, "close")
                .header(header::CACHE_CONTROL, "no-cache")
                .body(e.to_string().into())
                .map_err(|e| {
                    format_err!("failed to construct an HTTP error response: {}", e)
                        .compat()
                        .into()
                }),
            ErrorKind::Crit(e) => Err(e),
        }
    }
}
