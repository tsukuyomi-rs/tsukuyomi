//! [unstable]
//! Components for managing the session variables and storage.

use cookie::{Cookie, Key, PrivateJar};
use serde::de::DeserializeOwned;
use serde::ser::Serialize;
use serde_json;
use std::fmt;

use context::Context;
use error::Error;

/// A struct for managing the session variables.
pub struct SessionStorage {
    secret_key: Key,
}

impl fmt::Debug for SessionStorage {
    fn fmt(&self, f: &mut fmt::Formatter) -> fmt::Result {
        f.debug_struct("SessionStorage").finish()
    }
}

impl SessionStorage {
    /// Creates a builder object for constructing a value of this type.
    pub fn builder() -> Builder {
        Builder { secret_key: None }
    }

    /// Returns the reference to the secret key.
    pub(crate) fn secret_key(&self) -> &Key {
        &self.secret_key
    }
}

/// A builder object for constructing an instance of `SessionStorage`.
pub struct Builder {
    secret_key: Option<Key>,
}

impl fmt::Debug for Builder {
    fn fmt(&self, f: &mut fmt::Formatter) -> fmt::Result {
        f.debug_struct("Builder").finish()
    }
}

impl Builder {
    /// Generates a secret key to encrypt/decrypt Cookie values, by using the provided master key.
    pub fn secret_key<K>(&mut self, master_key: K) -> &mut Self
    where
        K: AsRef<[u8]>,
    {
        self.secret_key = Some(Key::from_master(master_key.as_ref()));
        self
    }

    /// Creates a new instance of `SessionStorage` with the current configuration.
    pub fn finish(&mut self) -> SessionStorage {
        SessionStorage {
            secret_key: self.secret_key.take().unwrap_or_else(|| Key::generate()),
        }
    }
}

/// A manager of session variables associated with the current request.
#[derive(Debug)]
pub struct Session<'a> {
    context: &'a Context,
}

impl<'a> Session<'a> {
    #[allow(missing_docs)]
    pub fn get<T>(&self, key: &str) -> Result<Option<T>, Error>
    where
        T: DeserializeOwned,
    {
        match self.with_private(|jar| jar.get(key))? {
            Some(cookie) => serde_json::from_str(cookie.value())
                .map(Some)
                .map_err(Error::bad_request),
            None => Ok(None),
        }
    }

    #[allow(missing_docs)]
    pub fn set<T>(&self, key: &str, value: T) -> Result<(), Error>
    where
        T: Serialize,
    {
        let value = serde_json::to_string(&value).map_err(Error::internal_server_error)?;
        let cookie = Cookie::new(key.to_owned(), value);
        self.with_private(|mut jar| jar.add(cookie))?;
        Ok(())
    }

    #[allow(missing_docs)]
    pub fn remove(&self, key: &str) -> Result<(), Error> {
        self.with_private(|mut jar| jar.remove(Cookie::named(key.to_owned())))
    }

    fn with_private<R>(&self, f: impl FnOnce(PrivateJar) -> R) -> Result<R, Error> {
        Ok(self.context
            .cookies()?
            .with_private(self.context.global().session().secret_key(), f))
    }
}

#[allow(missing_docs)]
pub trait ContextSessionExt {
    fn session(&self) -> Session;
}

impl ContextSessionExt for Context {
    fn session(&self) -> Session {
        Session { context: self }
    }
}
