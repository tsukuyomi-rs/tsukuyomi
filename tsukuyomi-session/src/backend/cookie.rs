#![allow(missing_docs)]

#[cfg(feature = "secure")]
use cookie::Key;
use {
    super::imp::{Backend, BackendImpl},
    cookie::Cookie,
    crate::{session::SessionInner, util::BuilderExt},
    serde_json,
    std::{borrow::Cow, collections::HashMap, fmt},
    time::Duration,
    tsukuyomi::{error::Result, handler::AsyncResult, input::Cookies},
};

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
    fn get(&self, name: &str, cookies: &mut Cookies<'_>) -> Result<Option<Cookie<'static>>> {
        match self {
            Security::Plain => Ok(cookies.jar()?.get(name).cloned()),
            #[cfg(feature = "secure")]
            Security::Signed(ref key) => Ok(cookies.signed_jar(key)?.get(name)),
            #[cfg(feature = "secure")]
            Security::Private(ref key) => Ok(cookies.private_jar(key)?.get(name)),
        }
    }

    fn add(&self, cookie: Cookie<'static>, cookies: &mut Cookies<'_>) -> Result<()> {
        match self {
            Security::Plain => cookies.jar()?.add(cookie),
            #[cfg(feature = "secure")]
            Security::Signed(ref key) => cookies.signed_jar(key)?.add(cookie),
            #[cfg(feature = "secure")]
            Security::Private(ref key) => cookies.private_jar(key)?.add(cookie),
        }
        Ok(())
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
        serde_json::from_str(s).map_err(tsukuyomi::error::bad_request)
    }

    fn serialize(&self, map: &HashMap<String, String>) -> String {
        serde_json::to_string(&map).expect("should be success")
    }
}

impl Backend for CookieSessionBackend {}
impl BackendImpl for CookieSessionBackend {
    fn read(&self) -> AsyncResult<SessionInner> {
        AsyncResult::ready(|input| {
            let this = input.states.try_get::<Self>().ok_or_else(|| {
                tsukuyomi::error::internal_server_error("the session backend is not set")
            })?;
            match this.security.get(&*this.cookie_name, input.cookies)? {
                Some(cookie) => {
                    let map = this.deserialize(cookie.value())?;
                    Ok(SessionInner::Some(map))
                }
                None => Ok(SessionInner::Empty),
            }
        })
    }

    fn write(&self, inner: SessionInner) -> AsyncResult<()> {
        AsyncResult::ready(move |input| {
            let this = input.states.try_get::<Self>().ok_or_else(|| {
                tsukuyomi::error::internal_server_error("the session backend is not set")
            })?;
            match inner {
                SessionInner::Empty => {}
                SessionInner::Some(map) => {
                    let value = this.serialize(&map);
                    let cookie = Cookie::build(this.cookie_name.clone(), value)
                        .if_some(this.max_age, |c, max_age| c.max_age(max_age))
                        .finish();
                    this.security.add(cookie, input.cookies)?;
                }
                SessionInner::Clear => {
                    input
                        .cookies
                        .jar()?
                        .force_remove(Cookie::named(this.cookie_name.clone()));
                }
            }

            Ok(())
        })
    }
}
