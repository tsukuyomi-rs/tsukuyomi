use {
    crate::{backend::Backend, session::SessionInner},
    futures::{try_ready, Async},
    std::marker::PhantomData,
    tsukuyomi::{handler::AsyncResult, modifier::Modifier, output::Output},
};

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

impl<B> tsukuyomi::app::Scope for SessionStorage<B>
where
    B: Backend + Send + Sync + 'static,
{
    type Error = tsukuyomi::Never;

    fn configure(self, cx: &mut tsukuyomi::app::scope::Context<'_>) -> Result<(), Self::Error> {
        cx.set_state(self.backend);
        cx.add_modifier(SessionModifier::<B>(PhantomData));
        Ok(())
    }
}

struct SessionModifier<B>(PhantomData<B>);

impl<B> Modifier for SessionModifier<B>
where
    B: Backend + Send + Sync + 'static,
{
    fn modify(&self, handle: AsyncResult<Output>) -> AsyncResult<Output> {
        enum State {
            Init(Option<AsyncResult<Output>>),
            Read {
                read_future: AsyncResult<SessionInner>,
                handle: Option<AsyncResult<Output>>,
            },
            InFlight(AsyncResult<Output>),
            Write {
                write_future: AsyncResult<()>,
                output: Option<Output>,
            },
        }

        let mut state = State::Init(Some(handle));

        AsyncResult::poll_fn(move |input| {
            let backend = input.states.try_get::<B>().ok_or_else(|| {
                tsukuyomi::error::internal_server_error("the session backend is not set")
            })?;

            loop {
                state = match state {
                    State::Init(ref mut handle) => State::Read {
                        read_future: backend.read(),
                        handle: handle.take(),
                    },
                    State::Read {
                        ref mut read_future,
                        ref mut handle,
                    } => {
                        let session_inner = try_ready!(read_future.poll_ready(input));
                        input.locals.insert(&SessionInner::KEY, session_inner);
                        State::InFlight(handle.take().unwrap())
                    }
                    State::InFlight(ref mut handle) => {
                        let output = try_ready!(handle.poll_ready(input));
                        let session_inner = input
                            .locals
                            .remove(&SessionInner::KEY)
                            .expect("should be Some");
                        State::Write {
                            write_future: backend.write(session_inner),
                            output: Some(output),
                        }
                    }
                    State::Write {
                        ref mut write_future,
                        ref mut output,
                    } => {
                        try_ready!(write_future.poll_ready(input));
                        let output = output.take().expect("the future has already polled");
                        return Ok(Async::Ready(output));
                    }
                };
            }
        })
    }
}
