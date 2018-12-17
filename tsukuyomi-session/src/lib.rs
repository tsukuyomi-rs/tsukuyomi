//! Session support for Tsukuyomi.

#![doc(html_root_url = "https://docs.rs/tsukuyomi-session/0.2.0-dev")]
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
        future::{MaybeDone, Poll, TryFuture},
        input::Input,
        responder::Responder,
    },
};

/// A trait representing the session backend.
#[allow(missing_docs)]
pub trait Backend {
    type Session: RawSession;
    type ReadSession: TryFuture<Ok = Self::Session, Error = Error> + Send + 'static;

    fn read(&self) -> Self::ReadSession;
}

impl<B> Backend for std::sync::Arc<B>
where
    B: Backend,
{
    type Session = B::Session;
    type ReadSession = B::ReadSession;

    fn read(&self) -> Self::ReadSession {
        (**self).read()
    }
}

#[allow(missing_docs)]
pub trait RawSession {
    type WriteSession: TryFuture<Ok = (), Error = Error> + Send + 'static;

    fn get(&self, name: &str) -> Option<&str>;
    fn set(&mut self, name: &str, value: String);
    fn remove(&mut self, name: &str);
    fn clear(&mut self);

    fn write(self) -> Self::WriteSession;
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
        Respond = impl TryFuture<Ok = T::Response, Error = Error> + Send + 'static,
    >
    where
        T: Responder,
        T::Respond: Send + 'static,
        T::Response: Send + 'static,
    {
        let mut write_session = MaybeDone::Pending(self.raw.write());
        let mut respond = MaybeDone::Pending(output.respond());

        tsukuyomi::responder::respond(tsukuyomi::future::poll_fn(move |input| {
            futures::try_ready!(write_session.poll_ready(input));
            futures::try_ready!(respond.poll_ready(input).map_err(Into::into));
            write_session
                .take_item()
                .expect("the future has already been polled.");
            let output = respond
                .take_item()
                .expect("the future has already been polled.");
            Ok(output.into())
        }))
    }
}

/// Create an `Extractor` which returns a `Session`.
pub fn session<B>(backend: B) -> SessionExtractor<B>
where
    B: Backend,
{
    SessionExtractor { backend }
}

#[doc(hidden)]
#[derive(Debug)]
pub struct SessionExtractor<B> {
    backend: B,
}

#[allow(clippy::type_complexity)]
impl<B> Extractor for SessionExtractor<B>
where
    B: Backend,
{
    type Output = (Session<B::Session>,);
    type Error = Error;
    type Extract = SessionExtract<B::ReadSession>;

    fn extract(&self) -> Self::Extract {
        SessionExtract {
            read_session: self.backend.read(),
        }
    }
}

#[doc(hidden)]
#[allow(missing_debug_implementations)]
pub struct SessionExtract<Fut> {
    read_session: Fut,
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
