use {
    crate::{error::HttpError, input::Input, output::Output},
    http::StatusCode,
    std::{error::Error as StdError, fmt},
};

/// A helper type which emulates the standard `never_type` (`!`).
#[cfg_attr(feature = "cargo-clippy", allow(empty_enum))]
#[derive(Debug)]
pub enum Never {}

impl fmt::Display for Never {
    fn fmt(&self, _: &mut fmt::Formatter<'_>) -> fmt::Result {
        match *self {}
    }
}

impl StdError for Never {
    fn description(&self) -> &str {
        match *self {}
    }

    fn cause(&self) -> Option<&dyn StdError> {
        match *self {}
    }
}

impl HttpError for Never {
    fn status_code(&self) -> StatusCode {
        match *self {}
    }

    fn to_response(&mut self, _: &mut Input<'_>) -> Output {
        match *self {}
    }
}
