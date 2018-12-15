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
    futures::Future,
    serde::{de::DeserializeOwned, ser::Serialize},
    tsukuyomi::{
        error::Error, //
        extractor::Extractor,
        input::Input,
        output::Responder,
    },
};

/// A trait representing the session backend.
#[allow(missing_docs)]
pub trait Backend {
    type Session: RawSession;
    type ReadSession: Future<Item = Self::Session, Error = Error> + Send + 'static;

    fn read(&self, input: &mut Input<'_>) -> Self::ReadSession;
}

impl<B> Backend for std::sync::Arc<B>
where
    B: Backend,
{
    type Session = B::Session;
    type ReadSession = B::ReadSession;

    fn read(&self, input: &mut Input<'_>) -> Self::ReadSession {
        (**self).read(input)
    }
}

#[allow(missing_docs)]
pub trait RawSession {
    type WriteSession: Future<Item = (), Error = Error> + Send + 'static;

    fn get(&self, name: &str) -> Option<&str>;
    fn set(&mut self, name: &str, value: String);
    fn remove(&mut self, name: &str);
    fn clear(&mut self);

    fn write(self, input: &mut Input<'_>) -> Self::WriteSession;
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
    pub fn finish<T>(self, output: T) -> impl Responder<Response = T::Response, Error = Error>
    where
        T: Responder,
        T::Response: Send + 'static,
    {
        tsukuyomi::output::respond(move |input| {
            let write_session = self.raw.write(input);
            let mut output = output.respond(input);
            write_session
                .join(futures::future::poll_fn(move || {
                    output.poll().map_err(Into::into)
                }))
                .map(move |((), output)| output)
        })
    }
}

/// Create an `Extractor` which returns a `Session`.
pub fn session<B>(backend: B) -> impl Extractor<Output = (Session<B::Session>,), Error = Error>
where
    B: Backend,
{
    tsukuyomi::extractor::raw(move |input| backend.read(input).map(|raw| (Session { raw },)))
}
