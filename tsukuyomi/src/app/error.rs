use {failure::Fail, std::fmt};

/// A type alias of `Result<T, E>` whose error type is restricted to `AppError`.
pub type Result<T> = std::result::Result<T, Error>;

/// An error type which will be thrown from `AppBuilder`.
#[derive(Debug)]
pub struct Error {
    compat: Compat,
}

impl fmt::Display for Error {
    #[inline]
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        self.compat.fmt(f)
    }
}

impl<E> From<E> for Error
where
    E: Into<failure::Error>,
{
    fn from(cause: E) -> Self {
        Self::custom(cause)
    }
}

impl Error {
    pub fn custom<E>(cause: E) -> Self
    where
        E: Into<failure::Error>,
    {
        Self {
            compat: Compat::Custom {
                cause: cause.into(),
            },
        }
    }

    pub fn compat(self) -> Compat {
        self.compat
    }
}

#[derive(Debug, Fail)]
pub enum Compat {
    #[fail(display = "{}", cause)]
    Custom { cause: failure::Error },
}
