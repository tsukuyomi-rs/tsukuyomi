//! Session support for Tsukuyomi.

#![doc(html_root_url = "https://docs.rs/tsukuyomi-session/0.2.0")]
#![deny(
    missing_docs,
    missing_debug_implementations,
    nonstandard_style,
    rust_2018_idioms,
    rust_2018_compatibility,
    unused
)]
#![forbid(clippy::unimplemented)]

pub mod backend;
mod util;

use {
    serde::{de::DeserializeOwned, ser::Serialize},
    tsukuyomi::{
        error::Error, //
        extractor::Extractor,
        future::{MaybeDone, TryFuture},
        responder::Responder,
    },
};

/// A trait representing the session backend.
pub trait Backend {
    /// The type of session which will be crated by `ReadSession`.
    type Session: RawSession;
    /// The type or errors which will occur when polling `ReadSession`.
    type ReadError: Into<Error>;
    /// The type of `TryFuture` that will return a `Session`.
    type ReadSession: TryFuture<Ok = Self::Session, Error = Self::ReadError>;

    /// Creates a `TryFuture` to create a `Session` asynchronously.
    fn read(&self) -> Self::ReadSession;
}

impl<B> Backend for Box<B>
where
    B: Backend,
{
    type Session = B::Session;
    type ReadError = B::ReadError;
    type ReadSession = B::ReadSession;

    #[inline]
    fn read(&self) -> Self::ReadSession {
        (**self).read()
    }
}

impl<B> Backend for std::rc::Rc<B>
where
    B: Backend,
{
    type Session = B::Session;
    type ReadError = B::ReadError;
    type ReadSession = B::ReadSession;

    #[inline]
    fn read(&self) -> Self::ReadSession {
        (**self).read()
    }
}

impl<B> Backend for std::sync::Arc<B>
where
    B: Backend,
{
    type Session = B::Session;
    type ReadError = B::ReadError;
    type ReadSession = B::ReadSession;

    #[inline]
    fn read(&self) -> Self::ReadSession {
        (**self).read()
    }
}

/// A trait that abstracts the management of session data during request handling.
pub trait RawSession {
    /// The error type during writing modification to the backend.
    type WriteError: Into<Error>;
    /// A `TryFuture` to write the modification of session data.
    type WriteSession: TryFuture<Ok = (), Error = Self::WriteError>;

    /// Returns the value of session data with the specified key name, if exists.
    fn get(&self, name: &str) -> Option<&str>;

    /// Appends a value to session data with the specified key name.
    fn set(&mut self, name: &str, value: String);

    /// Removes the value with the specified key name from session data.
    fn remove(&mut self, name: &str);

    /// Mark the session data as *cleared*.
    fn clear(&mut self);

    /// Consumes itself and creates a `TryFuture` to write the modification of session data.
    fn write(self) -> Self::WriteSession;
}

/// Create an `Extractor` which returns a `Session`.
pub fn session<B>(
    backend: B,
) -> impl Extractor<
    Output = (Session<B::Session>,),
    Error = B::ReadError,
    Extract = self::impl_extractor::SessionExtract<B::ReadSession>, // private
>
where
    B: Backend,
{
    tsukuyomi::extractor::extract(move || self::impl_extractor::SessionExtract {
        read_session: backend.read(),
    })
}

mod impl_extractor {
    use {
        super::{RawSession, Session},
        tsukuyomi::{
            future::{Poll, TryFuture},
            input::Input,
        },
    };

    #[allow(missing_debug_implementations)]
    pub struct SessionExtract<Fut> {
        pub(super) read_session: Fut,
    }

    impl<Fut> TryFuture for SessionExtract<Fut>
    where
        Fut: TryFuture,
        Fut::Ok: RawSession,
    {
        type Ok = (Session<Fut::Ok>,);
        type Error = Fut::Error;

        #[inline]
        fn poll_ready(&mut self, input: &mut Input<'_>) -> Poll<Self::Ok, Self::Error> {
            self.read_session
                .poll_ready(input)
                .map(|x| x.map(|raw| (Session { raw },)))
        }
    }
}

/// An interface of session values.
#[derive(Debug)]
pub struct Session<S: RawSession> {
    raw: S,
}

impl<S> Session<S>
where
    S: RawSession,
{
    /// Retrieves a field from this session and parses it into the specified type.
    pub fn get<T>(&self, name: &str) -> tsukuyomi::error::Result<Option<T>>
    where
        T: DeserializeOwned,
    {
        match self.raw.get(name) {
            Some(value) => serde_json::from_str(value)
                .map_err(tsukuyomi::error::internal_server_error)
                .map(Some),
            _ => Ok(None),
        }
    }

    /// Returns `true` if the field of specified name exists in this session.
    pub fn contains(&self, name: &str) -> bool {
        self.raw.get(name).is_some()
    }

    /// Sets a field to this session with serializing the specified value into a string.
    pub fn set<T>(&mut self, name: &str, value: T) -> tsukuyomi::error::Result<()>
    where
        T: Serialize,
    {
        let value = serde_json::to_string(&value) //
            .map_err(tsukuyomi::error::internal_server_error)?;
        self.raw.set(name, value);
        Ok(())
    }

    /// Removes a field from this session.
    pub fn remove(&mut self, name: &str) {
        self.raw.remove(name);
    }

    /// Marks this session cleared.
    pub fn clear(&mut self) {
        self.raw.clear();
    }

    /// Finalize the current session with the specified output.
    pub fn finish<T>(
        self,
        output: T,
    ) -> impl Responder<
        Response = T::Response,
        Error = Error,
        Respond = self::impl_responder::SessionRespond<S::WriteSession, T::Respond>, // private
    >
    where
        T: Responder,
    {
        tsukuyomi::responder::respond(self::impl_responder::SessionRespond {
            write_session: MaybeDone::Pending(self.raw.write()),
            respond: MaybeDone::Pending(output.respond()),
        })
    }
}

mod impl_responder {
    use tsukuyomi::{
        error::Error,
        future::{try_ready, MaybeDone, Poll, TryFuture},
        input::Input,
    };

    #[allow(missing_debug_implementations)]
    pub struct SessionRespond<S: TryFuture, T: TryFuture> {
        pub(super) write_session: MaybeDone<S>,
        pub(super) respond: MaybeDone<T>,
    }

    impl<S, T> TryFuture for SessionRespond<S, T>
    where
        S: TryFuture<Ok = ()>,
        T: TryFuture,
    {
        type Ok = T::Ok;
        type Error = Error;

        fn poll_ready(&mut self, input: &mut Input<'_>) -> Poll<Self::Ok, Self::Error> {
            try_ready!(self.write_session.poll_ready(input).map_err(Into::into));
            try_ready!(self.respond.poll_ready(input).map_err(Into::into));
            self.write_session
                .take_item()
                .expect("the future has already been polled.");
            let output = self
                .respond
                .take_item()
                .expect("the future has already been polled.");
            Ok(output.into())
        }
    }
}
