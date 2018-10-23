use cookie::{Cookie, CookieJar};
use http::header::{self, HeaderMap};
use std::marker::PhantomData;
use std::rc::Rc;

use crate::error::Error;

#[derive(Debug, Default)]
pub(crate) struct CookieManager {
    jar: Option<CookieJar>,
}

impl CookieManager {
    pub(crate) fn init(&mut self, h: &HeaderMap) -> Result<Cookies<'_>, Error> {
        if let Some(ref mut jar) = self.jar {
            Ok(Cookies {
                jar,
                _marker: PhantomData,
            })
        } else {
            let mut jar = CookieJar::new();
            for raw in h.get_all(header::COOKIE) {
                let raw_s = raw.to_str().map_err(crate::error::Failure::bad_request)?;
                for s in raw_s.split(';').map(|s| s.trim()) {
                    let cookie = Cookie::parse_encoded(s)
                        .map_err(crate::error::Failure::bad_request)?
                        .into_owned();
                    jar.add_original(cookie);
                }
            }
            Ok(Cookies {
                jar: self.jar.get_or_insert(jar),
                _marker: PhantomData,
            })
        }
    }

    pub(crate) fn append_to(&self, h: &mut HeaderMap) {
        if let Some(ref jar) = self.jar {
            for cookie in jar.delta() {
                h.insert(
                    header::SET_COOKIE,
                    cookie.encoded().to_string().parse().unwrap(),
                );
            }
        }
    }
}

/// A proxy object for accessing Cookie values.
///
/// Currently this type is a thin wrapper of `&mut cookie::CookieJar`.
#[derive(Debug)]
pub struct Cookies<'a> {
    jar: &'a mut CookieJar,
    _marker: PhantomData<Rc<()>>,
}

impl<'a> Cookies<'a> {
    /// Returns a reference to a Cookie value with the specified name.
    #[inline]
    pub fn get(&self, name: &str) -> Option<&Cookie<'static>> {
        self.jar.get(name)
    }

    /// Adds a Cookie entry into jar.
    #[inline]
    pub fn add(&mut self, cookie: Cookie<'static>) {
        self.jar.add(cookie);
    }

    /// Removes a Cookie entry from jar.
    #[inline]
    pub fn remove(&mut self, cookie: Cookie<'static>) {
        self.jar.remove(cookie);
    }

    /// Removes a Cookie entry *completely*.
    #[inline]
    pub fn force_remove(&mut self, cookie: Cookie<'static>) {
        self.jar.force_remove(cookie);
    }
}

#[cfg(feature = "secure")]
mod secure {
    use cookie::{Key, PrivateJar, SignedJar};

    impl<'a> super::Cookies<'a> {
        /// Creates a `SignedJar` with the specified secret key.
        #[inline]
        pub fn signed(&mut self, key: &Key) -> SignedJar<'_> {
            self.jar.signed(key)
        }

        /// Creates a `PrivateJar` with the specified secret key.
        #[inline]
        pub fn private(&mut self, key: &Key) -> PrivateJar<'_> {
            self.jar.private(key)
        }
    }
}
