use futures::{try_ready, Async};

use tsukuyomi::app::scope::Modifier;
use tsukuyomi::input::Input;
use tsukuyomi::output::Output;
use tsukuyomi::AsyncResult;

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
    fn before_handle(&self, input: &mut Input<'_>) -> AsyncResult<Option<Output>> {
        let mut read_future = self.backend.read(input);

        AsyncResult::polling(move |input| {
            let session_inner = try_ready!(read_future.poll_read(input));
            input.locals_mut().insert(&SessionInner::KEY, session_inner);
            Ok(Async::Ready(None))
        })
    }

    fn after_handle(
        &self,
        input: &mut Input<'_>,
        result: tsukuyomi::error::Result<Output>,
    ) -> AsyncResult<Output> {
        match result {
            Ok(output) => {
                let session_inner = input
                    .locals_mut()
                    .remove(&SessionInner::KEY)
                    .expect("should be Some");
                let mut write_future = self.backend.write(input, session_inner);
                let mut output_opt = Some(output);
                AsyncResult::polling(move |input| {
                    try_ready!(write_future.poll_write(input));
                    let output = output_opt.take().unwrap();
                    Ok(Async::Ready(output))
                })
            }
            Err(err) => AsyncResult::ready(Err(err)),
        }
    }
}
