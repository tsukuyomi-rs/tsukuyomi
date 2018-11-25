use {
    cookie::{Cookie, CookieBuilder},
    crate::{Backend, RawSession},
    serde_json,
    std::{borrow::Cow, collections::HashMap, fmt},
    tsukuyomi::{error::Result, input::Cookies, Input},
};

#[cfg(feature = "secure")]
use cookie::Key;

#[derive(Debug)]
pub struct CookieSession {
    inner: Inner,
}

#[derive(Debug)]
enum Inner {
    Empty,
    Some(HashMap<String, String>),
    Clear,
}

impl RawSession for CookieSession {
    fn get(&self, name: &str) -> Option<&str> {
        match self.inner {
            Inner::Some(ref map) => map.get(name).map(|s| &**s),
            _ => None,
        }
    }

    fn set(&mut self, name: &str, value: String) {
        match self.inner {
            Inner::Empty => {}
            Inner::Some(ref mut map) => {
                map.insert(name.to_owned(), value);
                return;
            }
            Inner::Clear => return,
        }

        match std::mem::replace(&mut self.inner, Inner::Empty) {
            Inner::Empty => {
                self.inner = Inner::Some({
                    let mut map = HashMap::new();
                    map.insert(name.to_owned(), value);
                    map
                });
            }
            Inner::Some(..) | Inner::Clear => unreachable!(),
        }
    }

    fn remove(&mut self, name: &str) {
        if let Inner::Some(ref mut map) = self.inner {
            map.remove(name);
        }
    }

    fn clear(&mut self) {
        self.inner = Inner::Clear;
    }
}

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

#[cfg_attr(tarpaulin, skip)]
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

/// A `Backend` using a Cookie entry for storing the session data.
#[cfg_attr(feature = "cargo-clippy", allow(stutter))]
pub struct CookieBackend {
    security: Security,
    cookie_name: Cow<'static, str>,
    builder: Box<dyn Fn(CookieBuilder) -> CookieBuilder + Send + Sync + 'static>,
}

#[cfg_attr(tarpaulin, skip)]
impl fmt::Debug for CookieBackend {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.debug_struct("CookieBackend")
            .field("security", &self.security)
            .field("cookie_name", &self.cookie_name)
            .finish()
    }
}

impl CookieBackend {
    fn new(security: Security) -> Self {
        Self {
            security,
            cookie_name: "tsukuyomi-session".into(),
            builder: Box::new(|cookie| cookie),
        }
    }

    /// Create a new `CookieBackend` that save uses the plain format.
    pub fn plain() -> Self {
        Self::new(Security::Plain)
    }

    /// Create a new `CookieBackend` that signs the cookie entry with the specified `Key`.
    #[cfg(feature = "secure")]
    pub fn signed(secret_key: Key) -> Self {
        Self::new(Security::Signed(secret_key))
    }

    /// Create a new `CookieBackend` that encrypts the cookie entry with the specified `Key`.
    #[cfg(feature = "secure")]
    pub fn private(secret_key: Key) -> Self {
        Self::new(Security::Private(secret_key))
    }

    /// Sets the name of Cookie entry to be used for storing the session data.
    ///
    /// The default value is `"tsukuyomi-session"`.
    pub fn cookie_name(self, value: impl Into<Cow<'static, str>>) -> Self {
        Self {
            cookie_name: value.into(),
            ..self
        }
    }

    /// Sets the functions for modifying the saved Cookie entry.
    pub fn builder(
        self,
        builder: impl Fn(CookieBuilder) -> CookieBuilder + Send + Sync + 'static,
    ) -> Self {
        Self {
            builder: Box::new(builder),
            ..self
        }
    }

    fn deserialize(&self, s: &str) -> Result<HashMap<String, String>> {
        serde_json::from_str(s).map_err(tsukuyomi::error::bad_request)
    }

    fn serialize(&self, map: &HashMap<String, String>) -> String {
        serde_json::to_string(&map).expect("should be success")
    }

    fn read_inner(&self, input: &mut Input<'_>) -> tsukuyomi::Result<CookieSession> {
        match self.security.get(&*self.cookie_name, input.cookies)? {
            Some(cookie) => {
                let map = self.deserialize(cookie.value())?;
                Ok(CookieSession {
                    inner: Inner::Some(map),
                })
            }
            None => Ok(CookieSession {
                inner: Inner::Empty,
            }),
        }
    }

    fn write_inner(&self, input: &mut Input<'_>, session: CookieSession) -> tsukuyomi::Result<()> {
        match session.inner {
            Inner::Empty => {}
            Inner::Some(map) => {
                let value = self.serialize(&map);
                let cookie =
                    (self.builder)(Cookie::build(self.cookie_name.clone(), value)).finish();
                self.security.add(cookie, input.cookies)?;
            }
            Inner::Clear => {
                input
                    .cookies
                    .jar()?
                    .remove(Cookie::named(self.cookie_name.clone()));
            }
        }

        Ok(())
    }
}

impl Backend for CookieBackend {
    type Session = CookieSession;
    type ReadSession = futures::future::FutureResult<Self::Session, tsukuyomi::Error>;
    type WriteSession = futures::future::FutureResult<(), tsukuyomi::Error>;

    fn read(&self, input: &mut Input<'_>) -> Self::ReadSession {
        futures::future::result(self.read_inner(input))
    }

    fn write(&self, input: &mut Input<'_>, session: Self::Session) -> Self::WriteSession {
        futures::future::result(self.write_inner(input, session))
    }
}
