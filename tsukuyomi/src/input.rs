//! Components for accessing the incoming request data.

pub mod body;
pub mod header;
pub mod localmap;
pub mod param;

use {
    self::{localmap::LocalMap, param::Params},
    cookie::{Cookie, CookieJar},
    http::{header::HeaderMap, Request},
    std::{marker::PhantomData, rc::Rc},
};

/// A proxy object for accessing the incoming HTTP request data.
#[derive(Debug)]
pub struct Input<'task> {
    /// The information of incoming request without the message body.
    pub request: &'task Request<()>,

    /// A set of extracted parameters from inner.
    pub params: &'task Option<Params<'task>>,

    /// A proxy object for accessing Cookie values.
    pub cookies: &'task mut Cookies<'task>,

    /// A typemap that holds arbitrary request-local inner.
    pub locals: &'task mut LocalMap,

    /// A map of header fields that will be inserted at reply to the client.
    pub response_headers: &'task mut Option<HeaderMap>,

    pub(crate) _marker: PhantomData<Rc<()>>,
}

/// A proxy object for accessing Cookie values.
#[derive(Debug)]
pub struct Cookies<'task> {
    jar: &'task mut Option<CookieJar>,
    request: &'task Request<()>,
    _marker: PhantomData<Rc<()>>,
}

impl<'task> Cookies<'task> {
    pub(crate) fn new(jar: &'task mut Option<CookieJar>, request: &'task Request<()>) -> Self {
        Self {
            jar,
            request,
            _marker: PhantomData,
        }
    }

    /// Returns the mutable reference to the inner `CookieJar` if available.
    pub fn jar(&mut self) -> crate::error::Result<&mut CookieJar> {
        if let Some(ref mut jar) = self.jar {
            return Ok(jar);
        }

        let jar = self.jar.get_or_insert_with(CookieJar::new);

        for raw in self.request.headers().get_all(http::header::COOKIE) {
            let raw_s = raw.to_str().map_err(crate::error::bad_request)?;
            for s in raw_s.split(';').map(|s| s.trim()) {
                let cookie = Cookie::parse_encoded(s)
                    .map_err(crate::error::bad_request)?
                    .into_owned();
                jar.add_original(cookie);
            }
        }

        Ok(jar)
    }
}

#[cfg(feature = "secure")]
mod secure {
    use crate::error::Result;
    use cookie::{Key, PrivateJar, SignedJar};

    impl<'a> super::Cookies<'a> {
        /// Creates a `SignedJar` with the specified secret key.
        #[inline]
        pub fn signed_jar(&mut self, key: &Key) -> Result<SignedJar<'_>> {
            Ok(self.jar()?.signed(key))
        }

        /// Creates a `PrivateJar` with the specified secret key.
        #[inline]
        pub fn private_jar(&mut self, key: &Key) -> Result<PrivateJar<'_>> {
            Ok(self.jar()?.private(key))
        }
    }
}
