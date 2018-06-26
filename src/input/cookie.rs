use cookie::{Cookie, CookieJar};
use failure::Error;
use http::header::{self, HeaderMap};

#[cfg(feature = "secure")]
use cookie::{Key, PrivateJar, SignedJar};

#[derive(Debug)]
pub(crate) struct CookieManager {
    jar: CookieJar,
    is_init: bool,
}

impl CookieManager {
    pub(crate) fn new() -> CookieManager {
        CookieManager {
            jar: CookieJar::new(),
            is_init: false,
        }
    }

    pub(crate) fn is_init(&self) -> bool {
        self.is_init
    }

    pub(crate) fn init(&mut self, h: &HeaderMap) -> Result<(), Error> {
        for raw in h.get_all(header::COOKIE) {
            let raw_s = raw.to_str()?;
            for s in raw_s.split(";").map(|s| s.trim()) {
                let cookie = Cookie::parse_encoded(s)?.into_owned();
                self.jar.add_original(cookie);
            }
        }

        self.is_init = true;

        Ok(())
    }

    pub(crate) fn cookies(&mut self) -> Cookies {
        Cookies { jar: &mut self.jar }
    }

    pub(crate) fn append_to(&self, h: &mut HeaderMap) {
        if !self.is_init {
            return;
        }

        for cookie in self.jar.delta() {
            h.insert(header::SET_COOKIE, cookie.encoded().to_string().parse().unwrap());
        }
    }
}

/// A proxy object for managing Cookie values.
///
/// This object is a thin wrapper of 'CookieJar' defined at 'cookie' crate,
/// and it provides some *basic* APIs for getting the entries of Cookies from an HTTP request
/// or adding/removing Cookie values into an HTTP response.
#[derive(Debug)]
pub struct Cookies<'a> {
    jar: &'a mut CookieJar,
}

impl<'a> Cookies<'a> {
    /// Gets a value of Cookie with the provided name from this jar.
    #[inline]
    pub fn get(&self, name: &str) -> Option<&Cookie<'static>> {
        self.jar.get(name)
    }

    /// Adds the provided entry of Cookie into this jar.
    #[inline]
    pub fn add(&mut self, cookie: Cookie<'static>) {
        self.jar.add(cookie)
    }

    /// Removes the provided entry of Cookie from this jar.
    #[inline]
    pub fn remove(&mut self, cookie: Cookie<'static>) {
        self.jar.remove(cookie)
    }

    /// Creates a proxy object to manage signed Cookies.
    ///
    /// This method is available only if the feature `session` is enabled.
    #[cfg(feature = "secure")]
    #[inline]
    pub fn signed(&mut self, key: &Key) -> SignedJar {
        self.jar.signed(key)
    }

    /// Creates a proxy object to manage encrypted Cookies.
    ///
    /// This method is available only if the feature `session` is enabled.
    #[cfg(feature = "secure")]
    #[inline]
    pub fn private(&mut self, key: &Key) -> PrivateJar {
        self.jar.private(key)
    }
}
