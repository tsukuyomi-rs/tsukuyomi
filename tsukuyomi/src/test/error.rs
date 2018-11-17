use std::fmt;

#[derive(Debug)]
pub struct Error {
    compat: Compat,
}

impl Error {
    pub fn compat(self) -> Compat {
        self.compat
    }
}

impl fmt::Display for Error {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        self.compat.fmt(f)
    }
}

#[derive(Debug, failure::Fail)]
pub enum Compat {
    #[fail(display = "app error: {}", _0)]
    App(crate::app::Error),
    #[fail(display = "custom error: {}", _0)]
    Custom(failure::Error),
}

impl<T> From<T> for Error
where
    T: Into<failure::Error>,
{
    fn from(err: T) -> Self {
        Self {
            compat: Compat::Custom(err.into()),
        }
    }
}

impl From<crate::app::Error> for Error {
    fn from(err: crate::app::Error) -> Self {
        Self {
            compat: Compat::App(err),
        }
    }
}

pub type Result<T> = std::result::Result<T, Error>;