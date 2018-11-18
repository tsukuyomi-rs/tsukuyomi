use {
    crate::{
        backend::{
            imp::{ReadFuture, WriteFuture},
            Backend,
        },
        session::SessionInner,
    },
    futures::{try_ready, Async},
    std::sync::Arc,
    tsukuyomi::{app::scope::Modifier, output::Output, AsyncResult},
};

/// A `Modifier` for managing session values.
#[derive(Debug)]
#[cfg_attr(feature = "cargo-clippy", allow(stutter))]
pub struct SessionStorage<B> {
    backend: Arc<B>,
}

impl<B> SessionStorage<B>
where
    B: Backend,
{
    /// Creates a `Storage` with the specified session backend.
    pub fn new(backend: B) -> Self {
        Self {
            backend: Arc::new(backend),
        }
    }
}

impl<B> Modifier for SessionStorage<B>
where
    B: Backend + Send + Sync + 'static,
{
    fn modify(&self, handle: AsyncResult<Output>) -> AsyncResult<Output> {
        enum State<R, W> {
            Init(Option<AsyncResult<Output>>),
            Read(R, Option<AsyncResult<Output>>),
            InFlight(AsyncResult<Output>),
            Write(W, Option<Output>),
        }

        let mut state = State::Init(Some(handle));
        let backend = self.backend.clone();

        AsyncResult::polling(move |input| loop {
            state = match state {
                State::Init(ref mut handle) => State::Read(backend.read(input), handle.take()),
                State::Read(ref mut read_future, ref mut handle) => {
                    let session_inner = try_ready!(read_future.poll_read(input));
                    input.locals_mut().insert(&SessionInner::KEY, session_inner);
                    State::InFlight(handle.take().unwrap())
                }
                State::InFlight(ref mut handle) => {
                    let output = try_ready!(handle.poll_ready(input));
                    let session_inner = input
                        .locals_mut()
                        .remove(&SessionInner::KEY)
                        .expect("should be Some");
                    State::Write(backend.write(input, session_inner), Some(output))
                }
                State::Write(ref mut write_future, ref mut output_opt) => {
                    try_ready!(write_future.poll_write(input));
                    let output = output_opt.take().expect("the future has already polled");
                    return Ok(Async::Ready(output));
                }
            };
        })
    }
}
