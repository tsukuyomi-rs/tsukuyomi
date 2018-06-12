use failure;
use fnv::FnvHashMap;
use http::Method;
use std::mem;

use error::Error;

use super::handler::Handler;
use super::recognizer::Recognizer;
use super::route::{normalize_uri, Route};

// TODO: treat trailing slashes
// TODO: fallback options

#[derive(Debug)]
struct Config {
    fallback_head: bool,
}

impl Default for Config {
    fn default() -> Self {
        Config { fallback_head: true }
    }
}

#[derive(Debug)]
struct RouterEntry {
    routes: FnvHashMap<Method, usize>,
}

impl RouterEntry {
    fn recognize(&self, method: &Method, config: &Config) -> Option<usize> {
        if let Some(&i) = self.routes.get(method) {
            return Some(i);
        }

        if config.fallback_head && *method == Method::GET {
            if let Some(&i) = self.routes.get(&Method::GET) {
                return Some(i);
            }
        }

        None
    }
}

/// An HTTP router.
#[derive(Debug)]
pub struct Router {
    recognizer: Recognizer<RouterEntry>,
    routes: Vec<Route>,
    config: Config,
}

impl Router {
    /// Creates a builder object for constructing a configured value of this type.
    pub fn builder() -> Builder {
        Builder {
            routes: vec![],
            config: None,
            result: Ok(()),
        }
    }

    /// Gets the reference to i-th `Route` contained in this router.
    pub fn get_route(&self, i: usize) -> Option<&Route> {
        self.routes.get(i)
    }

    /// Performs the routing and returns the index of a `Route` and a list of ranges representing
    /// the extracted value of parameters.
    pub fn recognize(&self, path: &str, method: &Method) -> Result<(usize, Vec<(usize, usize)>), Error> {
        let (entry, params) = self.recognizer.recognize(path).ok_or_else(|| Error::not_found())?;
        entry
            .recognize(method, &self.config)
            .map(|i| (i, params))
            .ok_or_else(|| Error::method_not_allowed())
    }
}

/// A builder object for constructing an instance of `Router`.
#[derive(Debug)]
pub struct Builder {
    routes: Vec<Route>,
    config: Option<Config>,
    result: Result<(), failure::Error>,
}

impl Builder {
    fn add_route<H>(&mut self, base: &str, path: &str, method: Method, handler: H) -> &mut Self
    where
        H: Handler + Send + Sync + 'static,
        H::Future: Send + 'static,
    {
        self.modify(move |self_| {
            let base = normalize_uri(base)?;
            let path = normalize_uri(path)?;
            self_.routes.push(Route::new(base, path, method, handler));
            Ok(())
        })
    }

    /// Creates a proxy object to add some routes mounted to the provided prefix.
    pub fn mount<'a>(&'a mut self, base: &'a str) -> Mount<'a> {
        Mount {
            builder: self,
            base: base,
        }
    }

    /// Sets whether the fallback to GET if the handler for HEAD is not registered is enabled or not.
    ///
    /// The default value is `true`.
    pub fn fallback_head(&mut self, enabled: bool) -> &mut Self {
        self.modify(move |self_| {
            self_.config.get_or_insert_with(Default::default).fallback_head = enabled;
            Ok(())
        })
    }

    fn modify(&mut self, f: impl FnOnce(&mut Self) -> Result<(), failure::Error>) -> &mut Self {
        if self.result.is_ok() {
            self.result = f(self);
        }
        self
    }

    /// Creates an instance of `Router` with current configuration.
    pub fn finish(&mut self) -> Result<Router, failure::Error> {
        let Builder { routes, config, result } = mem::replace(self, Router::builder());

        result?;

        let config = config.unwrap_or_default();

        let mut res: FnvHashMap<String, FnvHashMap<Method, usize>> = FnvHashMap::with_hasher(Default::default());
        for (i, route) in routes.iter().enumerate() {
            res.entry(route.full_path())
                .or_insert_with(Default::default)
                .insert(route.method().clone(), i);
        }

        let mut builder = Recognizer::builder();
        for (path, routes) in res {
            builder.insert(&path, RouterEntry { routes: routes });
        }

        let recognizer = builder.finish()?;

        Ok(Router {
            recognizer: recognizer,
            routes: routes,
            config: config,
        })
    }
}

/// A proxy object for adding routes with the certain prefix.
#[derive(Debug)]
pub struct Mount<'a> {
    builder: &'a mut Builder,
    base: &'a str,
}

macro_rules! impl_methods_for_mount {
    ($(
        $(#[$doc:meta])*
        $name:ident => $METHOD:ident,
    )*) => {$(
        $(#[$doc])*
        #[inline]
        pub fn $name<H>(&mut self, path: &str, handler: H) -> &mut Self
        where
            H: Handler + Send + Sync + 'static,
            H::Future: Send + 'static,
        {
            self.route(path, Method::$METHOD, handler)
        }
    )*};
}

impl<'a> Mount<'a> {
    /// Adds a route with the provided path, method and handler.
    pub fn route<H>(&mut self, path: &str, method: Method, handler: H) -> &mut Self
    where
        H: Handler + Send + Sync + 'static,
        H::Future: Send + 'static,
    {
        self.builder.add_route(self.base, path, method, handler);
        self
    }

    impl_methods_for_mount![
        /// Equivalent to `mount.route(path, Method::GET, handler)`.
        get => GET,

        /// Equivalent to `mount.route(path, Method::POST, handler)`.
        post => POST,

        /// Equivalent to `mount.route(path, Method::PUT, handler)`.
        put => PUT,

        /// Equivalent to `mount.route(path, Method::DELETE, handler)`.
        delete => DELETE,

        /// Equivalent to `mount.route(path, Method::HEAD, handler)`.
        head => HEAD,

        /// Equivalent to `mount.route(path, Method::OPTIONS, handler)`.
        options => OPTIONS,

        /// Equivalent to `mount.route(path, Method::PATCH, handler)`.
        patch => PATCH,
    ];
}
