use crate::{
    error::{Critical, Error},
    input::Input,
    output::Output,
};

pub trait Callback: Send + Sync + 'static {
    #[allow(unused_variables)]
    fn on_init(&self, input: &mut Input<'_>) -> Result<Option<Output>, Error> {
        Ok(None)
    }

    fn on_error(&self, err: Error, input: &mut Input<'_>) -> Result<Output, Critical> {
        err.into_response(input)
    }
}

impl Callback for () {}
