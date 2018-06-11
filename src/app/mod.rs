//! The definition of components for constructing the HTTP applications.

pub mod service;

use failure::Error;
use std::sync::Arc;
use std::{fmt, mem};

#[cfg(feature = "session")]
use cookie::Key;

use error::handler::{DefaultErrorHandler, ErrorHandler};
use router::{self, Mount, Router};

scoped_thread_local!(static STATE: AppState);

/// The global and shared variables used throughout the serving an HTTP application.
pub struct AppState {
    router: Router,
    error_handler: Box<ErrorHandler + Send + Sync + 'static>,
    #[cfg(feature = "session")]
    secret_key: Key,
}

impl fmt::Debug for AppState {
    fn fmt(&self, f: &mut fmt::Formatter) -> fmt::Result {
        f.debug_struct("AppState").field("router", &self.router).finish()
    }
}

impl AppState {
    pub(crate) fn set<R>(&self, f: impl FnOnce() -> R) -> R {
        STATE.set(self, f)
    }

    /// Returns `true` if the reference to a `AppState` is set to the scoped TLS.
    pub fn is_set() -> bool {
        STATE.is_set()
    }

    /// Executes a closure by using the reference to `AppState` set to the scoped TLS and
    /// returns its result.
    pub fn with<R>(f: impl FnOnce(&AppState) -> R) -> R {
        STATE.with(f)
    }

    /// Returns the reference to `Router` contained in this value.
    pub fn router(&self) -> &Router {
        &self.router
    }

    /// Returns the reference to `ErrorHandler` contained in this value.
    pub fn error_handler(&self) -> &ErrorHandler {
        &*self.error_handler
    }

    /// Returns the reference to the secret key contained in this value.
    ///
    /// This method is available only if the feature `session` is enabled.
    #[cfg(feature = "session")]
    pub fn secret_key(&self) -> &Key {
        &self.secret_key
    }
}

/// The main type in this framework, which represents an HTTP application.
#[derive(Debug)]
pub struct App {
    state: Arc<AppState>,
}

impl App {
    /// Creates a builder object for constructing an instance of this type.
    pub fn builder() -> AppBuilder {
        AppBuilder {
            router: Router::builder(),
            error_handler: None,
            #[cfg(feature = "session")]
            secret_key: None,
        }
    }
}

/// A builder object for constructing an instance of `App`.
pub struct AppBuilder {
    router: router::Builder,
    error_handler: Option<Box<ErrorHandler + Send + Sync + 'static>>,
    #[cfg(feature = "session")]
    secret_key: Option<Key>,
}

impl fmt::Debug for AppBuilder {
    fn fmt(&self, f: &mut fmt::Formatter) -> fmt::Result {
        f.debug_struct("AppBuilder").field("router", &self.router).finish()
    }
}

impl AppBuilder {
    /// Registers some handlers to the router, with mounting on the specified prefix.
    ///
    /// See the documentation of `Mount` for details.
    ///
    /// # Examples
    ///
    /// ```
    /// # use ganymede::{App, Context};
    /// # let index = |_: &Context| Ok("a");
    /// # let find_post = |_: &Context| Ok("a");
    /// # let all_posts = |_: &Context| Ok("a");
    /// # let add_post = |_: &Context| Ok("a");
    /// let app = App::builder()
    ///     .mount("/", |r| { r.get("/", index); })
    ///     .mount("/api/v1/", |r| {
    ///         r.get("/posts/:id", find_post);
    ///         r.get("/posts", all_posts);
    ///         r.post("/posts", add_post);
    ///     })
    ///     .finish();
    /// ```
    pub fn mount(&mut self, base: &str, f: impl FnOnce(&mut Mount)) -> &mut Self {
        f(&mut self.router.mount(base));
        self
    }

    /// Modifies the router level configurations.
    pub fn router(&mut self, f: impl FnOnce(&mut router::Builder)) -> &mut Self {
        f(&mut self.router);
        self
    }

    /// Sets the instance to an error handler into this builder.
    pub fn error_handler<H>(&mut self, error_handler: H) -> &mut Self
    where
        H: ErrorHandler + Send + Sync + 'static,
    {
        self.error_handler = Some(Box::new(error_handler));
        self
    }

    /// Generates a secret key for encrypting the Cookie values from the provided master key.
    ///
    /// This method is available only if the feature `session` is enabled.
    #[cfg(feature = "session")]
    pub fn secret_key<K>(&mut self, master_key: K) -> &mut Self
    where
        K: AsRef<[u8]>,
    {
        self.secret_key = Some(Key::from_master(master_key.as_ref()));
        self
    }

    /// Creates a configured `App` from the current configuration.
    pub fn finish(&mut self) -> Result<App, Error> {
        let mut builder = mem::replace(self, App::builder());

        let state = AppState {
            router: builder.router.finish()?,
            error_handler: builder
                .error_handler
                .unwrap_or_else(|| Box::new(DefaultErrorHandler::new())),
            #[cfg(feature = "session")]
            secret_key: builder.secret_key.unwrap_or_else(Key::generate),
        };

        Ok(App { state: Arc::new(state) })
    }
}
