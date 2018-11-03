use std::collections::HashMap;

use futures::try_ready;
use futures::Async;
use serde::de::DeserializeOwned;
use serde::ser::Serialize;
use serde_json;

use tsukuyomi::error::{Failure, Result};
use tsukuyomi::input::Input;
use tsukuyomi::local_key;
use tsukuyomi::modifier::{AfterHandle, BeforeHandle, Modifier};
use tsukuyomi::output::Output;

use crate::backend::imp::{Backend, ReadFuture, WriteFuture};

/// A `Modifier` for managing session values.
#[derive(Debug)]
pub struct Storage<B> {
    backend: B,
}

impl<B> Storage<B>
where
    B: Backend,
{
    /// Creates a `Storage` with the specified session backend.
    pub fn new(backend: B) -> Self {
        Self { backend }
    }
}

impl<B> Modifier for Storage<B>
where
    B: Backend,
{
    fn before_handle(&self, input: &mut Input<'_>) -> BeforeHandle {
        let mut read_future = self.backend.read(input);

        BeforeHandle::polling(move |input| {
            let state = try_ready!(read_future.poll_read(input));

            input.locals_mut().insert(&Session::KEY, Session { state });

            Ok(Async::Ready(None))
        })
    }

    fn after_handle(&self, input: &mut Input<'_>, result: Result<Output>) -> AfterHandle {
        match result {
            Ok(output) => {
                let session = input
                    .locals_mut()
                    .remove(&Session::KEY)
                    .expect("should be Some");

                let mut write_future = self.backend.write(input, session.state);

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

/// An interface of session values.
#[derive(Debug)]
pub struct Session {
    state: SessionState,
}

#[cfg_attr(feature = "cargo-clippy", allow(stutter))]
#[derive(Debug)]
pub enum SessionState {
    Empty,
    Some(HashMap<String, String>),
    Clear,
}

impl Session {
    local_key!(const KEY: Self);

    /// Retrieves a field from this session and parses it into the specified type.
    pub fn get<T>(&self, name: &str) -> Result<Option<T>>
    where
        T: DeserializeOwned,
    {
        match self.state {
            SessionState::Some(ref map) => match map.get(name) {
                Some(s) => serde_json::from_str(s)
                    .map_err(Failure::internal_server_error)
                    .map_err(Into::into)
                    .map(Some),
                None => Ok(None),
            },
            _ => Ok(None),
        }
    }

    /// Sets a field to this session with serializing the specified value into a string.
    pub fn set<T>(&mut self, name: &str, value: T) -> Result<()>
    where
        T: Serialize,
    {
        let value = serde_json::to_string(&value).map_err(Failure::internal_server_error)?;

        match self.state {
            SessionState::Empty => {}
            SessionState::Some(ref mut map) => {
                map.insert(name.to_owned(), value);
                return Ok(());
            }
            SessionState::Clear => return Ok(()),
        }

        match std::mem::replace(&mut self.state, SessionState::Empty) {
            SessionState::Empty => {
                self.state = SessionState::Some({
                    let mut map = HashMap::new();
                    map.insert(name.to_owned(), value);
                    map
                });
            }
            SessionState::Some(..) | SessionState::Clear => unreachable!(),
        }
        Ok(())
    }

    /// Removes a field from this session.
    pub fn remove(&mut self, name: &str) {
        if let SessionState::Some(ref mut map) = self.state {
            map.remove(name);
        }
    }

    /// Marks this session cleared.
    pub fn clear(&mut self) {
        self.state = SessionState::Clear;
    }
}

/// An extension trait which adding methods for accessing `Session`.
#[cfg_attr(feature = "cargo-clippy", allow(stutter))]
pub trait InputSessionExt: Sealed {
    #[allow(missing_docs)]
    fn session(&mut self) -> Option<&mut Session>;
}

impl<'a> InputSessionExt for Input<'a> {
    fn session(&mut self) -> Option<&mut Session> {
        self.locals_mut().get_mut(&Session::KEY)
    }
}

pub trait Sealed {}
impl<'a> Sealed for Input<'a> {}
