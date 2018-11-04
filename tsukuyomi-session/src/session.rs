use std::collections::HashMap;

use serde::de::DeserializeOwned;
use serde::ser::Serialize;
use serde_json;

use tsukuyomi::extractor::{Extract, Extractor, HasExtractor};
use tsukuyomi::input::local_map::local_key;
use tsukuyomi::input::Input;

#[cfg_attr(feature = "cargo-clippy", allow(stutter))]
#[derive(Debug)]
pub enum SessionInner {
    Empty,
    Some(HashMap<String, String>),
    Clear,
}

impl SessionInner {
    local_key!(pub(crate) const KEY: Self);
}

/// An interface of session values.
#[derive(Debug)]
pub struct Session {
    _priv: (),
}

impl Session {
    fn with_inner<R>(&self, f: impl FnOnce(&SessionInner) -> R) -> R {
        tsukuyomi::input::with_get_current(|input| {
            let inner = input
                .locals()
                .get(&SessionInner::KEY)
                .expect("should be exist");
            f(inner)
        })
    }

    fn with_inner_mut<R>(&mut self, f: impl FnOnce(&mut SessionInner) -> R) -> R {
        tsukuyomi::input::with_get_current(|input| {
            let inner = input
                .locals_mut()
                .get_mut(&SessionInner::KEY)
                .expect("should be exist");
            f(inner)
        })
    }

    /// Retrieves a field from this session and parses it into the specified type.
    pub fn get<T>(&self, name: &str) -> tsukuyomi::error::Result<Option<T>>
    where
        T: DeserializeOwned,
    {
        self.with_inner(|inner| match inner {
            SessionInner::Some(ref map) => match map.get(name) {
                Some(s) => serde_json::from_str(s)
                    .map_err(tsukuyomi::error::internal_server_error)
                    .map(Some),
                None => Ok(None),
            },
            _ => Ok(None),
        })
    }

    /// Returns `true` if the field of specified name exists in this session.
    pub fn contains(&self, name: &str) -> bool {
        self.with_inner(|inner| {
            if let SessionInner::Some(ref map) = inner {
                map.contains_key(name)
            } else {
                false
            }
        })
    }

    /// Sets a field to this session with serializing the specified value into a string.
    pub fn set<T>(&mut self, name: &str, value: T) -> tsukuyomi::error::Result<()>
    where
        T: Serialize,
    {
        self.with_inner_mut(|inner| {
            let value =
                serde_json::to_string(&value).map_err(tsukuyomi::error::internal_server_error)?;

            match inner {
                SessionInner::Empty => {}
                SessionInner::Some(ref mut map) => {
                    map.insert(name.to_owned(), value);
                    return Ok(());
                }
                SessionInner::Clear => return Ok(()),
            }

            match std::mem::replace(inner, SessionInner::Empty) {
                SessionInner::Empty => {
                    *inner = SessionInner::Some({
                        let mut map = HashMap::new();
                        map.insert(name.to_owned(), value);
                        map
                    });
                }
                SessionInner::Some(..) | SessionInner::Clear => unreachable!(),
            }
            Ok(())
        })
    }

    /// Removes a field from this session.
    pub fn remove(&mut self, name: &str) {
        self.with_inner_mut(|inner| {
            if let SessionInner::Some(ref mut map) = inner {
                map.remove(name);
            }
        })
    }

    /// Marks this session cleared.
    pub fn clear(&mut self) {
        self.with_inner_mut(|inner| {
            *inner = SessionInner::Clear;
        })
    }
}

impl HasExtractor for Session {
    type Extractor = SessionExtractor;

    fn extractor() -> Self::Extractor {
        SessionExtractor { _priv: () }
    }
}

#[derive(Debug)]
#[cfg_attr(feature = "cargo-clippy", allow(stutter))]
pub struct SessionExtractor {
    _priv: (),
}

impl Extractor for SessionExtractor {
    type Output = (Session,);
    type Error = tsukuyomi::error::Error;
    type Future = tsukuyomi::extractor::Placeholder<Self::Output, Self::Error>;

    fn extract(&self, input: &mut Input<'_>) -> Result<Extract<Self>, Self::Error> {
        if input.locals_mut().contains_key(&SessionInner::KEY) {
            Ok(Extract::Ready((Session { _priv: () },)))
        } else {
            Err(tsukuyomi::error::internal_server_error(
                "The session is not available at the current scope.",
            ))
        }
    }
}
