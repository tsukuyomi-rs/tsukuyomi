//! Session support for Tsukuyomi.

#![doc(html_root_url = "https://docs.rs/tsukuyomi-session/0.1.0")]
#![warn(
    missing_docs,
    missing_debug_implementations,
    nonstandard_style,
    rust_2018_idioms,
    rust_2018_compatibility,
    unused
)]
#![cfg_attr(tsukuyomi_deny_warnings, deny(warnings))]
#![cfg_attr(tsukuyomi_deny_warnings, doc(test(attr(deny(warnings)))))]
#![cfg_attr(feature = "cargo-clippy", warn(pedantic))]
#![cfg_attr(feature = "cargo-clippy", allow(stutter))]
#![cfg_attr(feature = "cargo-clippy", forbid(unimplemented))]

extern crate cookie;
extern crate futures;
extern crate serde;
extern crate serde_json;
extern crate tsukuyomi;

#[cfg(feature = "redis-backend")]
extern crate redis;
#[cfg(feature = "redis-backend")]
extern crate uuid;

pub mod backend;
mod util;

use {
    futures::{try_ready, Async, Future},
    serde::{de::DeserializeOwned, ser::Serialize},
    std::{any::TypeId, fmt, sync::Arc},
    tsukuyomi::{
        error::Error, //
        extractor::Extractor,
        handler::AsyncResult,
        localmap::local_key,
        output::responder,
        Input,
        Modifier,
        Output,
        Responder,
    },
};

#[allow(missing_docs)]
pub trait RawSession: Send + 'static {
    fn get(&self, name: &str) -> Option<&str>;
    fn set(&mut self, name: &str, value: String);
    fn remove(&mut self, name: &str);
    fn clear(&mut self);

    #[doc(hidden)]
    fn __private_type_id__(&self) -> TypeId {
        TypeId::of::<Self>()
    }
}

/// A trait representing the session backend.
#[allow(missing_docs)]
pub trait Backend: Send + Sync + 'static {
    type Session: RawSession;
    type ReadSession: Future<Item = Self::Session, Error = Error> + Send + 'static;
    type WriteSession: Future<Item = (), Error = Error> + Send + 'static;

    fn read(&self, input: &mut Input<'_>) -> Self::ReadSession;
    fn write(&self, input: &mut Input<'_>, data: Self::Session) -> Self::WriteSession;
}

#[allow(keyword_idents)]
local_key! {
    static RAW_SESSION: Box<dyn RawSession>;
}

/// An interface of session values.
pub struct Session {
    raw: Box<dyn RawSession>,
}

#[cfg_attr(tarpaulin, skip)]
impl fmt::Debug for Session {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.debug_struct("Session").finish()
    }
}

impl Session {
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
    pub fn finish<T>(self, output: T) -> impl Responder
    where
        T: Responder,
    {
        responder(|input| -> Result<_, T::Error> {
            output
                .respond_to(input) //
                .map(|output| {
                    input.locals.insert(&RAW_SESSION, self.raw);
                    output
                })
        })
    }
}

/// Create an `Extractor` which returns a `Session`.
pub fn session() -> impl Extractor<Output = (Session,), Error = Error> {
    tsukuyomi::extractor::ready(|input| {
        if let Some(raw) = input.locals.remove(&RAW_SESSION) {
            Ok(Session { raw })
        } else {
            Err(tsukuyomi::error::internal_server_error(
                "The session is not available at the current scope.",
            ))
        }
    })
}

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
    B: Backend,
{
    fn modify(&self, handle: AsyncResult<Output>) -> AsyncResult<Output> {
        enum State<R, W> {
            Init(Option<AsyncResult<Output>>),
            Read {
                read_session: R,
                handle: Option<AsyncResult<Output>>,
            },
            InFlight(AsyncResult<Output>),
            Write {
                write_session: W,
                output: Option<Output>,
            },
        }

        let mut state = State::Init(Some(handle));
        let backend = self.backend.clone();

        AsyncResult::poll_fn(move |input| {
            loop {
                state = match state {
                    State::Init(ref mut handle) => State::Read {
                        read_session: backend.read(input),
                        handle: handle.take(),
                    },
                    State::Read {
                        ref mut read_session,
                        ref mut handle,
                    } => {
                        let raw = try_ready!(read_session.poll());
                        input.locals.insert(&RAW_SESSION, Box::new(raw));
                        State::InFlight(handle.take().unwrap())
                    }
                    State::InFlight(ref mut handle) => {
                        let output = try_ready!(handle.poll_ready(input));
                        let raw = input
                            .locals
                            .remove(&RAW_SESSION) //
                            .ok_or_else(|| {
                                tsukuyomi::error::internal_server_error(
                                    "the session has not finished yet.",
                                )
                            })?;

                        if raw.__private_type_id__() == TypeId::of::<B::Session>() {
                            let raw =
                                unsafe { *Box::from_raw(Box::into_raw(raw) as *mut B::Session) };
                            State::Write {
                                write_session: backend.write(input, raw),
                                output: Some(output),
                            }
                        } else {
                            return Err(tsukuyomi::error::internal_server_error(
                                "failed to downcast the raw session.",
                            ));
                        }
                    }
                    State::Write {
                        ref mut write_session,
                        ref mut output,
                    } => {
                        try_ready!(write_session.poll());
                        let output = output.take().expect("the future has already polled");
                        return Ok(Async::Ready(output));
                    }
                };
            }
        })
    }
}
