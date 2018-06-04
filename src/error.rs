use failure;
use http::{header, Response, StatusCode};
use std::error::Error as StdError;

use response::ResponseBody;

pub type CritError = Box<StdError + Send + Sync + 'static>;

#[derive(Debug)]
pub enum ErrorKind {
    Failed(failure::Error),
    Crit(CritError),
    // TODO: add more variant
}

#[derive(Debug)]
pub struct Error {
    kind: ErrorKind,
}

impl<E> From<E> for Error
where
    E: Into<failure::Error>,
{
    fn from(err: E) -> Error {
        Error::failed(err)
    }
}

impl Error {
    pub fn failed<E>(err: E) -> Error
    where
        E: Into<failure::Error>,
    {
        Error {
            kind: ErrorKind::Failed(err.into()),
        }
    }

    pub fn critical<E>(err: E) -> Error
    where
        E: StdError + Send + Sync + 'static,
    {
        Error {
            kind: ErrorKind::Crit(Box::new(err)),
        }
    }

    pub fn kind(&self) -> &ErrorKind {
        &self.kind
    }

    pub fn is_critical(&self) -> bool {
        match self.kind {
            ErrorKind::Crit(..) => true,
            _ => false,
        }
    }

    pub fn into_response(self) -> Result<Response<ResponseBody>, CritError> {
        match self.kind {
            ErrorKind::Failed(e) => Response::builder()
                .status(StatusCode::INTERNAL_SERVER_ERROR)
                .header(header::CONNECTION, "close")
                .header(header::CACHE_CONTROL, "no-cache")
                .body(e.to_string().into())
                .map_err(Into::into),
            ErrorKind::Crit(e) => Err(e),
        }
    }
}
