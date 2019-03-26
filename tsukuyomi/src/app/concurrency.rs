//! Specification of trait object's concurrency used in `App`.

pub mod current_thread;

use {
    crate::{
        error::Error,
        future::{Async, Poll, TryFuture},
        handler::Handler,
        input::Input,
        output::Response,
        responder::{Respond, Responder},
        upgrade::{Error as UpgradeError, Upgrade, Upgraded},
        util::Never,
    },
    std::fmt,
};

/// A trait to specify the concurrency of trait objects inside of `AppBase`.
pub trait Concurrency: Sized + 'static {
    type Handler;

    #[doc(hidden)]
    type Impl: self::imp::ConcurrencyImpl<Concurrency = Self>;
}

pub(super) mod imp {
    use {
        super::Concurrency,
        crate::{
            input::Input,
            output::Response,
            upgrade::{Error as UpgradeError, Upgraded},
        },
        futures01::Poll,
    };

    pub trait ConcurrencyImpl: Sized + 'static {
        type Concurrency: Concurrency<Impl = Self>;
        type Handle;
        type Upgrade;

        fn handle(handler: &<Self::Concurrency as Concurrency>::Handler) -> Self::Handle;

        fn poll_ready_handle(
            handle: &mut Self::Handle,
            input: &mut Input<'_>,
        ) -> Poll<(Response, Option<Self::Upgrade>), crate::error::Error>;

        fn poll_upgrade(conn: &mut Self::Upgrade, io: &mut Upgraded<'_>) -> Poll<(), UpgradeError>;
        fn close_upgrade(conn: &mut Self::Upgrade);
    }
}

/// A `Conccurency` used in `App` by default.
#[allow(missing_debug_implementations)]
pub struct DefaultConcurrency(Never);

impl Concurrency for DefaultConcurrency {
    #[doc(hidden)]
    type Impl = Self;
    type Handler = BoxedHandler;
}

impl self::imp::ConcurrencyImpl for DefaultConcurrency {
    type Concurrency = Self;
    type Handle = Box<BoxedHandle>;
    type Upgrade = Box<dyn Upgrade + Send>;

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
    dyn FnMut(
            &mut Input<'_>,
        ) -> Poll<(Response, Option<Box<dyn Upgrade + Send>>), crate::error::Error>
        + Send
        + 'static;

pub struct BoxedHandler(Box<dyn Fn() -> Box<BoxedHandle> + Send + Sync + 'static>);

impl fmt::Debug for BoxedHandler {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.debug_struct("BoxedHandler").finish()
    }
}

impl<H> From<H> for BoxedHandler
where
    H: Handler + Send + Sync + 'static,
    H::Handle: Send + 'static,
    H::Output: Responder,
    <H::Output as Responder>::Respond: Send + 'static,
    <H::Output as Responder>::Upgrade: Send + 'static,
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

                        let up = up.map(|up| Box::new(up) as Box<dyn Upgrade + Send>);

                        return Ok(Async::Ready((res, up)));
                    }
                };
            })
        }))
    }
}
