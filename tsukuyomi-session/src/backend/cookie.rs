use {
    crate::{Backend, RawSession},
    cookie::{Cookie, CookieBuilder},
    serde_json,
    std::{borrow::Cow, collections::HashMap, fmt, sync::Arc},
    tsukuyomi::{
        error::{Error, Result},
        future::{Poll, TryFuture},
        input::{Cookies, Input},
    },
};

#[cfg(feature = "secure")]
use cookie::Key;

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
#[derive(Debug, Clone)]
pub struct CookieBackend {
    inner: Arc<CookieBackendInner>,
}

impl CookieBackend {
    fn new(security: Security) -> Self {
        Self {
            inner: Arc::new(CookieBackendInner {
                security,
                cookie_name: "tsukuyomi-session".into(),
                builder: Box::new(|cookie| cookie),
            }),
        }
    }

    fn inner_mut(&mut self) -> &mut CookieBackendInner {
        Arc::get_mut(&mut self.inner).expect("the instance has already shared")
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
    pub fn cookie_name(mut self, value: impl Into<Cow<'static, str>>) -> Self {
        self.inner_mut().cookie_name = value.into();
        self
    }

    /// Sets the functions for modifying the saved Cookie entry.
    pub fn builder(
        mut self,
        builder: impl Fn(CookieBuilder) -> CookieBuilder + Send + Sync + 'static,
    ) -> Self {
        self.inner_mut().builder = Box::new(builder);
        self
    }
}

struct CookieBackendInner {
    security: Security,
    cookie_name: Cow<'static, str>,
    builder: Box<dyn Fn(CookieBuilder) -> CookieBuilder + Send + Sync + 'static>,
}

#[cfg_attr(tarpaulin, skip)]
impl fmt::Debug for CookieBackendInner {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.debug_struct("CookieBackendInner")
            .field("security", &self.security)
            .field("cookie_name", &self.cookie_name)
            .finish()
    }
}

impl CookieBackendInner {
    fn deserialize(&self, s: &str) -> Result<HashMap<String, String>> {
        serde_json::from_str(s).map_err(tsukuyomi::error::bad_request)
    }

    fn serialize(&self, map: &HashMap<String, String>) -> String {
        serde_json::to_string(&map).expect("should be success")
    }

    fn read(&self, input: &mut Input<'_>) -> tsukuyomi::Result<Inner> {
        match self.security.get(&*self.cookie_name, input.cookies)? {
            Some(cookie) => {
                let map = self.deserialize(cookie.value())?;
                Ok(Inner::Some(map))
            }
            None => Ok(Inner::Empty),
        }
    }

    fn write(&self, input: &mut Input<'_>, inner: Inner) -> tsukuyomi::Result<()> {
        match inner {
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
    type ReadError = Error;
    type ReadSession = ReadSession;

    fn read(&self) -> Self::ReadSession {
        ReadSession(Some(self.clone()))
    }
}

#[doc(hidden)]
#[allow(missing_debug_implementations)]
pub struct ReadSession(Option<CookieBackend>);

impl TryFuture for ReadSession {
    type Ok = CookieSession;
    type Error = Error;

    #[inline]
    fn poll_ready(&mut self, input: &mut Input<'_>) -> Poll<Self::Ok, Self::Error> {
        let backend = self.0.take().expect("the future has already been polled");
        backend
            .inner
            .read(input)
            .map(|inner| CookieSession { inner, backend }.into())
    }
}

#[derive(Debug)]
pub struct CookieSession {
    inner: Inner,
    backend: CookieBackend,
}

#[derive(Debug)]
enum Inner {
    Empty,
    Some(HashMap<String, String>),
    Clear,
}

impl RawSession for CookieSession {
    type WriteSession = WriteSession;
    type WriteError = Error;

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

    fn write(self) -> Self::WriteSession {
        WriteSession(Some(self))
    }
}

#[doc(hidden)]
#[allow(missing_debug_implementations)]
pub struct WriteSession(Option<CookieSession>);

impl TryFuture for WriteSession {
    type Ok = ();
    type Error = Error;

    #[inline]
    fn poll_ready(&mut self, input: &mut Input<'_>) -> Poll<Self::Ok, Self::Error> {
        let session = self.0.take().expect("the future has already been polled");
        session
            .backend
            .inner
            .write(input, session.inner)
            .map(Into::into)
    }
}
