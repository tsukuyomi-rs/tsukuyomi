use {
    super::Concurrency,
    crate::{
        error::Error,
        future::{Async, Poll, TryFuture},
        handler::Handler,
        input::Input,
        output::{Respond, Responder, Response},
        upgrade::{Error as UpgradeError, Upgrade, Upgraded},
        util::Never,
    },
    std::fmt,
};

/// The implementor of `Concurrency` which means that `App` is *not* thread safe.
#[allow(missing_debug_implementations)]
pub struct CurrentThread(Never);

impl Concurrency for CurrentThread {
    #[doc(hidden)]
    type Impl = Self;
    type Handler = BoxedHandler;
}

impl super::imp::ConcurrencyImpl for CurrentThread {
    type Concurrency = Self;
    type Handle = Box<BoxedHandle>;
    type Upgrade = Box<dyn Upgrade>;

    fn handle(handler: &<Self::Concurrency as Concurrency>::Handler) -> Self::Handle {
        (handler.0)()
    }

    fn poll_ready_handle(
        handle: &mut Self::Handle,
        input: &mut Input<'_>,
    ) -> Poll<(Response, Option<Self::Upgrade>), Error> {
        (handle)(input)
    }

    fn poll_upgrade(conn: &mut Self::Upgrade, io: &mut Upgraded<'_>) -> Poll<(), UpgradeError> {
        conn.poll_upgrade(io)
    }

    fn close_upgrade(conn: &mut Self::Upgrade) {
        conn.close();
    }
}

type BoxedHandle =
    dyn FnMut(&mut Input<'_>) -> Poll<(Response, Option<Box<dyn Upgrade>>), crate::error::Error>
        + 'static;

pub struct BoxedHandler(Box<dyn Fn() -> Box<BoxedHandle> + 'static>);

impl fmt::Debug for BoxedHandler {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.debug_struct("BoxedHandler").finish()
    }
}

impl<H> From<H> for BoxedHandler
where
    H: Handler + 'static,
    H::Handle: 'static,
    H::Output: Responder,
    <H::Output as Responder>::Respond: 'static,
    <H::Output as Responder>::Upgrade: 'static,
{
    fn from(handler: H) -> Self {
        BoxedHandler(Box::new(move || {
            enum State<A, B> {
                First(A),
                Second(B),
            }

            let mut state: State<H::Handle, <H::Output as Responder>::Respond> =
                State::First(handler.handle());

            Box::new(move |input| loop {
                state = match state {
                    State::First(ref mut handle) => {
                        let x = futures01::try_ready!(handle.poll_ready(input).map_err(Into::into));
                        State::Second(x.respond())
                    }
                    State::Second(ref mut respond) => {
                        let (res, up) =
                            futures01::try_ready!(respond.poll_respond(input).map_err(Into::into));

                        let up = up.map(|up| Box::new(up) as Box<dyn Upgrade>);

                        return Ok(Async::Ready((res, up)));
                    }
                };
            })
        }))
    }
}
