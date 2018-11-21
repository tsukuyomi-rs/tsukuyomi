use crate::{
    error::{Critical, Error},
    input::Input,
    output::Output,
};

pub trait ErrorHandler {
    fn call(&self, err: Error, input: &mut Input<'_>) -> Result<Output, Critical> {
        err.into_response(input)
    }
}

impl ErrorHandler for () {}
