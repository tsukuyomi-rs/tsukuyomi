//! Components for building an `App`.

use std::sync::Arc;
use std::{fmt, mem};

use failure::{Error, Fail};
use fnv::FnvHashMap;
use http::{HttpTryFrom, Method};
use state::Container;

use error::handler::{DefaultErrorHandler, ErrorHandler};
use handler::Handler;
use modifier::Modifier;

use super::endpoint::Endpoint;
use super::recognizer::Recognizer;
use super::uri::{self, Uri};
use super::{App, AppState, Config, RouterEntry};

/// A builder object for constructing an instance of `App`.
pub struct AppBuilder {
    endpoints: Vec<Endpoint>,
    error_handler: Option<Box<dyn ErrorHandler + Send + Sync + 'static>>,
    modifiers: Vec<Box<dyn Modifier + Send + Sync + 'static>>,
    states: Container,
    config: Option<Config>,

    result: Result<(), Error>,
}

#[cfg_attr(tarpaulin, skip)]
impl fmt::Debug for AppBuilder {
    fn fmt(&self, f: &mut fmt::Formatter) -> fmt::Result {
        f.debug_struct("AppBuilder").finish()
    }
}

impl AppBuilder {
    pub(super) fn new() -> AppBuilder {
        AppBuilder {
            endpoints: vec![],
            error_handler: None,
            modifiers: vec![],
            states: Container::new(),
            config: None,

            result: Ok(()),
        }
    }

    /// Creates a proxy object to add some routes mounted to the provided prefix.
    ///
    /// # Examples
    ///
    /// ```
    /// # use tsukuyomi::app::App;
    /// # use tsukuyomi::input::Input;
    /// # use tsukuyomi::handler::Handler;
    /// # use tsukuyomi::output::Responder;
    /// fn index (_: &mut Input) -> impl Responder {
    ///     // ...
    /// #   "index"
    /// }
    /// # fn find_post (_:&mut Input) -> &'static str { "a" }
    /// # fn all_posts (_:&mut Input) -> &'static str { "a" }
    /// # fn add_post (_:&mut Input) -> &'static str { "a" }
    ///
    /// # fn main() -> tsukuyomi::AppResult<()> {
    /// let app = App::builder()
    ///     .mount("/", |m| {
    ///         m.get("/").handle(Handler::new_ready(index));
    ///     })
    ///     .mount("/api/v1/", |m| {
    ///         m.get("/posts/:id").handle(Handler::new_ready(find_post));
    ///         m.get("/posts").handle(Handler::new_ready(all_posts));
    ///         m.post("/posts").handle(Handler::new_ready(add_post));
    ///     })
    ///     .finish()?;
    /// # Ok(())
    /// # }
    /// ```
    pub fn mount(&mut self, base: &str, f: impl FnOnce(&mut Mount)) -> &mut Self {
        let mut prefix = vec![];
        self.modify(|_| {
            prefix.push(Uri::from_str(base)?);
            Ok(())
        });

        f(&mut Mount {
            builder: self,
            prefix: prefix,
        });

        self
    }

    /// Sets whether the fallback to GET if the handler for HEAD is not registered is enabled or not.
    ///
    /// The default value is `true`.
    pub fn fallback_head(&mut self, enabled: bool) -> &mut Self {
        self.modify(move |self_| {
            self_.config.get_or_insert_with(Default::default).fallback_head = enabled;
            Ok(())
        });
        self
    }

    /// Sets whether the fallback to default OPTIONS handler if not registered is enabled or not.
    ///
    /// The default value is `false`.
    pub fn fallback_options(&mut self, enabled: bool) -> &mut Self {
        self.modify(move |self_| {
            self_.config.get_or_insert_with(Default::default).fallback_options = enabled;
            Ok(())
        });
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

    /// Sets the instance to an error handler into this builder.
    pub fn modifier<M>(&mut self, modifier: M) -> &mut Self
    where
        M: Modifier + Send + Sync + 'static,
    {
        self.modifiers.push(Box::new(modifier));
        self
    }

    /// Sets a value of `T` to the global storage.
    ///
    /// If a value of provided type has already set, this method drops `state` immediately
    /// and does not provide any affects to the global storage.
    pub fn manage<T>(&mut self, state: T) -> &mut Self
    where
        T: Send + Sync + 'static,
    {
        self.states.set(state);
        self
    }

    /// Creates a configured `App` using the current settings.
    pub fn finish(&mut self) -> Result<App, Error> {
        let AppBuilder {
            endpoints,
            config,
            result,
            error_handler,
            modifiers,
            mut states,
        } = mem::replace(self, AppBuilder::new());

        result?;

        let config = config.unwrap_or_default();
        let mut entries = vec![];

        let recognizer = {
            let mut collected_routes = FnvHashMap::with_hasher(Default::default());
            for (i, endpoint) in endpoints.iter().enumerate() {
                collected_routes
                    .entry(endpoint.uri())
                    .or_insert_with(RouterEntry::builder)
                    .push(endpoint.method(), i);
            }

            let mut builder = Recognizer::builder();
            for (path, entry) in collected_routes {
                builder.push(path.as_ref())?;
                entries.push(entry.finish()?);
            }

            builder.finish()
        };

        let error_handler = error_handler.unwrap_or_else(|| Box::new(DefaultErrorHandler::new()));

        states.freeze();

        let global = AppState {
            recognizer: recognizer,
            entries: entries,
            endpoints: endpoints,
            config: config,
            error_handler: error_handler,
            modifiers: modifiers,
            states: states,
        };

        Ok(App {
            global: Arc::new(global),
        })
    }

    fn modify(&mut self, f: impl FnOnce(&mut Self) -> Result<(), Error>) {
        if self.result.is_ok() {
            self.result = f(self);
        }
    }
}

/// A proxy object for adding routes with the certain prefix.
#[derive(Debug)]
pub struct Mount<'a> {
    builder: &'a mut AppBuilder,
    prefix: Vec<Uri>,
}

macro_rules! impl_methods_for_mount {
    ($(
        $(#[$doc:meta])*
        $name:ident => $METHOD:ident,
    )*) => {$(
        $(#[$doc])*
        #[inline]
        pub fn $name<'b>(&'b mut self, path: &str) -> Route<'a, 'b>
        where
            'a: 'b,
        {
            self.route(path, Method::$METHOD)
        }
    )*};
}

impl<'a> Mount<'a> {
    /// Adds a route with the provided path, method and handler.
    pub fn route<'b>(&'b mut self, path: &str, method: Method) -> Route<'a, 'b>
    where
        'a: 'b,
    {
        let mut suffix = Uri::new();
        self.builder.modify(|_| {
            suffix = Uri::from_str(path)?;
            Ok(())
        });
        Route {
            mount: self,
            suffix: suffix,
            method: method,
        }
    }

    #[allow(missing_docs)]
    pub fn mount(&mut self, base: &str, f: impl FnOnce(&mut Mount)) {
        let mut prefix = self.prefix.clone();
        self.builder.modify(|_| {
            prefix.push(Uri::from_str(base)?);
            Ok(())
        });
        let mut mount = Mount {
            builder: self.builder,
            prefix: prefix,
        };
        f(&mut mount);
    }

    impl_methods_for_mount![
        /// Equivalent to `mount.route(path, Method::GET)`.
        get => GET,

        /// Equivalent to `mount.route(path, Method::POST)`.
        post => POST,

        /// Equivalent to `mount.route(path, Method::PUT)`.
        put => PUT,

        /// Equivalent to `mount.route(path, Method::DELETE)`.
        delete => DELETE,

        /// Equivalent to `mount.route(path, Method::HEAD)`.
        head => HEAD,

        /// Equivalent to `mount.route(path, Method::OPTIONS)`.
        options => OPTIONS,

        /// Equivalent to `mount.route(path, Method::PATCH)`.
        patch => PATCH,
    ];
}

/// A proxy object for creating an endpoint from a handler function.
#[derive(Debug)]
pub struct Route<'a: 'b, 'b> {
    mount: &'b mut Mount<'a>,
    suffix: Uri,
    method: Method,
}

impl<'a, 'b> Route<'a, 'b> {
    /// Modifies the HTTP method associated with this endpoint.
    pub fn method<M>(&mut self, method: M) -> &mut Self
    where
        Method: HttpTryFrom<M>,
        <Method as HttpTryFrom<M>>::Error: Fail,
    {
        self.mount.builder.modify({
            let m = &mut self.method;
            move |_| {
                *m = Method::try_from(method)?;
                Ok(())
            }
        });
        self
    }

    /// Modifies the suffix URI of this endpoint.
    pub fn path(&mut self, path: &str) -> &mut Self {
        self.mount.builder.modify({
            let suffix = &mut self.suffix;
            move |_| {
                *suffix = Uri::from_str(path)?;
                Ok(())
            }
        });
        self
    }

    /// Finishes this session and registers an endpoint with given handler.
    pub fn handle(self, handler: impl Into<Handler>) {
        let uri = uri::join_all(self.mount.prefix.iter().chain(Some(&self.suffix)));
        let endpoint = Endpoint::new(uri, self.method, handler.into());
        self.mount.builder.endpoints.push(endpoint);
    }
}
