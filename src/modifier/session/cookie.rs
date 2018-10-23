use std::borrow::Cow;
use std::fmt;

use cookie::Cookie;
#[cfg(feature = "secure")]
use cookie::Key;
use time::Duration;

use crate::error::Result;
use crate::input::{Cookies, Input};
use crate::modifier::{AfterHandle, BeforeHandle, Modifier};
use crate::output::Output;
use crate::util::BuilderExt;

use super::{Session, SessionState};

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

    fn before_handle_inner(&self, input: &mut Input<'_>) -> Result<Option<Output>> {
        let session = {
            let mut cookies = input.cookies()?;
            match self.security.get(&*self.cookie_name, &mut cookies) {
                Some(cookie) => {
                    let map = serde_json::from_str(cookie.value())
                        .map_err(crate::error::Failure::bad_request)?;
                    Session::some(map)
                }
                None => Session::empty(),
            }
        };
        input.locals_mut().insert(&Session::KEY, session);
        Ok(None)
    }

    fn after_handle_inner(&self, input: &mut Input<'_>, res: Result<Output>) -> Result<Output> {
        if res.is_ok() {
            let values = input
                .locals_mut()
                .remove(&Session::KEY)
                .expect("should be exist");
            let mut cookies = input.cookies()?;
            match values.state {
                SessionState::Empty => {}
                SessionState::Some(values) => {
                    let value = serde_json::to_string(&values).expect("should be success");
                    let cookie = Cookie::build(self.cookie_name.clone(), value)
                        .if_some(self.max_age, |this, max_age| this.max_age(max_age))
                        .finish();
                    self.security.add(cookie, &mut cookies);
                }
                SessionState::Clear => {
                    cookies.force_remove(Cookie::named(self.cookie_name.clone()));
                }
            }
        }
        res
    }
}

impl Modifier for CookieSessionBackend {
    fn before_handle(&self, input: &mut Input<'_>) -> BeforeHandle {
        BeforeHandle::ready(self.before_handle_inner(input))
    }

    fn after_handle(&self, input: &mut Input<'_>, res: Result<Output>) -> AfterHandle {
        AfterHandle::ready(self.after_handle_inner(input, res))
    }
}
