#![allow(missing_docs)]

use std::borrow::Cow;
use std::collections::HashMap;
use std::fmt;

use cookie::Cookie;
#[cfg(feature = "secure")]
use cookie::Key;
use futures::{future, IntoFuture};
use serde_json;
use time::Duration;

use tsukuyomi::error::{Error, Result};
use tsukuyomi::input::{Cookies, Input};

use super::imp::{Backend, BackendImpl};
use crate::session::SessionState;
use crate::util::BuilderExt;

#[cfg(feature = "secure")]
enum Security {
    Plain,
    Signed(Key),
    Private(Key),
}

#[cfg(not(feature = "secure"))]
enum Security {
    Plain,
}

impl fmt::Debug for Security {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Security::Plain => f.debug_tuple("Plain").finish(),
            #[cfg(feature = "secure")]
            Security::Signed(..) => f.debug_tuple("Signed").field(&"<secret key>").finish(),
            #[cfg(feature = "secure")]
            Security::Private(..) => f.debug_tuple("Private").field(&"<secret key>").finish(),
        }
    }
}

impl Security {
    fn get(&self, name: &str, cookies: &mut Cookies<'_>) -> Option<Cookie<'static>> {
        match self {
            Security::Plain => cookies.get(name).cloned(),
            #[cfg(feature = "secure")]
            Security::Signed(ref key) => cookies.signed(key).get(name),
            #[cfg(feature = "secure")]
            Security::Private(ref key) => cookies.private(key).get(name),
        }
    }

    fn add(&self, cookie: Cookie<'static>, cookies: &mut Cookies<'_>) {
        match self {
            Security::Plain => cookies.add(cookie),
            #[cfg(feature = "secure")]
            Security::Signed(ref key) => cookies.signed(key).add(cookie),
            #[cfg(feature = "secure")]
            Security::Private(ref key) => cookies.private(key).add(cookie),
        }
    }
}

#[cfg_attr(feature = "cargo-clippy", allow(stutter))]
#[derive(Debug)]
pub struct CookieSessionBackend {
    security: Security,
    cookie_name: Cow<'static, str>,
    max_age: Option<Duration>,
}

impl CookieSessionBackend {
    fn new(security: Security) -> Self {
        Self {
            security,
            cookie_name: "session".into(),
            max_age: None,
        }
    }

    pub fn plain() -> Self {
        Self::new(Security::Plain)
    }

    #[cfg(feature = "secure")]
    pub fn signed(secret_key: Key) -> Self {
        Self::new(Security::Signed(secret_key))
    }

    #[cfg(feature = "secure")]
    pub fn private(secret_key: Key) -> Self {
        Self::new(Security::Private(secret_key))
    }

    pub fn cookie_name(self, value: impl Into<Cow<'static, str>>) -> Self {
        Self {
            cookie_name: value.into(),
            ..self
        }
    }

    pub fn max_age(self, value: Duration) -> Self {
        Self {
            max_age: Some(value),
            ..self
        }
    }

    fn deserialize(&self, s: &str) -> Result<HashMap<String, String>> {
        serde_json::from_str(s)
            .map_err(tsukuyomi::error::Failure::bad_request)
            .map_err(Into::into)
    }

    fn serialize(&self, map: HashMap<String, String>) -> String {
        serde_json::to_string(&map).expect("should be success")
    }

    fn read(&self, input: &mut Input<'_>) -> Result<SessionState> {
        let mut cookies = input.cookies()?;
        match self.security.get(&*self.cookie_name, &mut cookies) {
            Some(cookie) => {
                let map = self.deserialize(cookie.value())?;
                Ok(SessionState::Some(map))
            }
            None => Ok(SessionState::Empty),
        }
    }

    fn write(&self, input: &mut Input<'_>, state: SessionState) -> Result<()> {
        let mut cookies = input.cookies()?;
        match state {
            SessionState::Empty => {}
            SessionState::Some(map) => {
                let value = self.serialize(map);
                let cookie = Cookie::build(self.cookie_name.clone(), value)
                    .if_some(self.max_age, |this, max_age| this.max_age(max_age))
                    .finish();
                self.security.add(cookie, &mut cookies);
            }
            SessionState::Clear => {
                cookies.force_remove(Cookie::named(self.cookie_name.clone()));
            }
        }
        Ok(())
    }
}

impl Backend for CookieSessionBackend {}
impl BackendImpl for CookieSessionBackend {
    type ReadFuture = future::FutureResult<SessionState, Error>;
    type WriteFuture = future::FutureResult<(), Error>;

    fn read(&self, input: &mut Input<'_>) -> Self::ReadFuture {
        self.read(input).into_future()
    }

    fn write(&self, input: &mut Input<'_>, state: SessionState) -> Self::WriteFuture {
        self.write(input, state).into_future()
    }
}
