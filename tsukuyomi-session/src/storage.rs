use futures::{try_ready, Async};

use tsukuyomi::input::Input;
use tsukuyomi::modifier::{AfterHandle, BeforeHandle, Modifier};
use tsukuyomi::output::Output;

use crate::backend::imp::{ReadFuture, WriteFuture};
use crate::backend::Backend;
use crate::session::SessionInner;

/// A `Modifier` for managing session values.
#[derive(Debug)]
#[cfg_attr(feature = "cargo-clippy", allow(stutter))]
pub struct SessionStorage<B> {
    backend: B,
}

impl<B> SessionStorage<B>
where
    B: Backend,
{
    /// Creates a `Storage` with the specified session backend.
    pub fn new(backend: B) -> Self {
        Self { backend }
    }
}

impl<B> Modifier for SessionStorage<B>
where
    B: Backend,
{
    fn before_handle(&self, input: &mut Input<'_>) -> BeforeHandle {
        let mut read_future = self.backend.read(input);

        BeforeHandle::polling(move |input| {
            let session_inner = try_ready!(read_future.poll_read(input));
            input.locals_mut().insert(&SessionInner::KEY, session_inner);
            Ok(Async::Ready(None))
        })
    }

    fn after_handle(
        &self,
        input: &mut Input<'_>,
        result: tsukuyomi::error::Result<Output>,
    ) -> AfterHandle {
        match result {
            Ok(output) => {
                let session_inner = input
                    .locals_mut()
                    .remove(&SessionInner::KEY)
                    .expect("should be Some");
                let mut write_future = self.backend.write(input, session_inner);
                let mut output_opt = Some(output);
                AfterHandle::polling(move |input| {
                    try_ready!(write_future.poll_write(input));
                    let output = output_opt.take().unwrap();
                    Ok(Async::Ready(output))
                })
            }
            Err(err) => AfterHandle::ready(Err(err)),
        }
    }
}
