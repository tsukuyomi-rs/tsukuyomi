use failure::Fail;
use std::fmt;

/// A type alias of `Result<T, E>` whose error type is restricted to `AppError`.
#[cfg_attr(feature = "cargo-clippy", allow(stutter))]
pub type AppResult<T> = std::result::Result<T, AppError>;

/// An error type which will be thrown from `AppBuilder`.
#[cfg_attr(feature = "cargo-clippy", allow(stutter))]
#[derive(Debug)]
pub struct AppError {
    kind: AppErrorKind,
}

impl fmt::Display for AppError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        self.kind.fmt(f)
    }
}

impl<E> From<E> for AppError
where
    E: Into<failure::Error>,
{
    fn from(cause: E) -> Self {
        Self::custom(cause)
    }
}

impl AppError {
    pub fn custom<E>(cause: E) -> Self
    where
        E: Into<failure::Error>,
    {
        Self {
            kind: AppErrorKind::Custom {
                cause: cause.into(),
            },
        }
    }

    pub fn into_kind(self) -> AppErrorKind {
        self.kind
    }
}

#[derive(Debug, Fail)]
pub enum AppErrorKind {
    #[fail(display = "{}", cause)]
    Custom { cause: failure::Error },
}
