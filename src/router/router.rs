use failure::{self, Fail};
use fnv::FnvHashMap;
use http::header::HeaderValue;
use http::{header, HttpTryFrom, Method, Response};
use std::collections::HashSet;
use std::mem;
use std::ops::Index;

use error::Error;
use handler::{Handle, Handler};
use input::Input;

use super::endpoint::Endpoint;
use super::recognizer::Recognizer;
use super::uri::{self, Uri};

// TODO: treat trailing slashes

#[derive(Debug)]
struct Config {
    fallback_head: bool,
    fallback_options: bool,
}

impl Default for Config {
    fn default() -> Self {
        Config {
            fallback_head: true,
            fallback_options: false,
        }
    }
}

// ==== RouterEntry ====

#[derive(Debug)]
struct RouterEntry {
    routes: FnvHashMap<Method, usize>,
    allowed_methods: HeaderValue,
}

impl RouterEntry {
    fn builder() -> RouterEntryBuilder {
        RouterEntryBuilder {
            routes: vec![],
            methods: HashSet::new(),
        }
    }

    fn get(&self, method: &Method) -> Option<usize> {
        self.routes.get(method).map(|&i| i)
    }

    fn allowed_methods(&self) -> HeaderValue {
        self.allowed_methods.clone()
    }
}

#[derive(Debug)]
struct RouterEntryBuilder {
    routes: Vec<(Method, usize)>,
    methods: HashSet<Method>,
}

impl RouterEntryBuilder {
    fn push(&mut self, method: &Method, i: usize) {
        self.routes.push((method.clone(), i));
        self.methods.insert(method.clone());
    }

    fn finish(self) -> Result<RouterEntry, failure::Error> {
        let RouterEntryBuilder { routes, mut methods } = self;

        methods.insert(Method::OPTIONS);
        let allowed_methods = methods.into_iter().fold(String::new(), |mut acc, method| {
            if !acc.is_empty() {
                acc += ", ";
            }
            acc += method.as_ref();
            acc
        });

        Ok(RouterEntry {
            routes: routes.into_iter().collect(),
            allowed_methods: HeaderValue::from_shared(allowed_methods.into())?,
        })
    }
}

// ==== Router ====

#[derive(Debug)]
enum Recognize {
    Matched(usize, Vec<(usize, usize)>),
    Options(HeaderValue),
}

/// An HTTP router.
#[derive(Debug)]
pub struct Router {
    recognizer: Recognizer<RouterEntry>,
    endpoints: Vec<Endpoint>,
    config: Config,
}

impl Router {
    /// Creates a builder object for constructing a configured value of this type.
    pub fn builder() -> Builder {
        Builder {
            endpoints: vec![],
            config: None,
            result: Ok(()),
        }
    }

    /// Gets the reference to i-th `Route` contained in this router.
    pub fn get(&self, i: usize) -> Option<&Endpoint> {
        self.endpoints.get(i)
    }

    fn recognize(&self, path: &str, method: &Method) -> Result<Recognize, Error> {
        let (entry, params) = self.recognizer.recognize(path).ok_or_else(|| Error::not_found())?;
        match entry.get(method) {
            Some(i) => Ok(Recognize::Matched(i, params)),
            None if self.config.fallback_head && *method == Method::HEAD => match entry.get(&Method::GET) {
                Some(i) => Ok(Recognize::Matched(i, params)),
                None => Err(Error::method_not_allowed()),
            },
            None if self.config.fallback_options && *method == Method::OPTIONS => {
                Ok(Recognize::Options(entry.allowed_methods()))
            }
            None => Err(Error::method_not_allowed()),
        }
    }

    pub(crate) fn handle(&self, input: &mut Input) -> Result<Handle, Error> {
        match self.recognize(input.uri().path(), input.method())? {
            Recognize::Matched(i, params) => {
                input.parts.route = Some((i, params));
                let endpoint = &self.endpoints[i];
                Ok(endpoint.handler().handle(input))
            }
            Recognize::Options(allowed_methods) => {
                let mut response = Response::new(());
                response.headers_mut().insert(header::ALLOW, allowed_methods);
                Ok(Handle::ok(response.into()))
            }
        }
    }
}

impl Index<usize> for Router {
    type Output = Endpoint;

    fn index(&self, i: usize) -> &Self::Output {
        self.get(i).expect("out of range")
    }
}

/// A builder object for constructing an instance of `Router`.
#[derive(Debug)]
pub struct Builder {
    endpoints: Vec<Endpoint>,
    config: Option<Config>,
    result: Result<(), failure::Error>,
}

impl Builder {
    /// Creates a proxy object to add some routes mounted to the provided prefix.
    ///
    /// # Examples
    ///
    /// ```
    /// # use tsukuyomi::input::Input;
    /// # use tsukuyomi::handler::Handler;
    /// # use tsukuyomi::output::Responder;
    /// # use tsukuyomi::router::Router;
    /// fn index (_: &mut Input) -> impl Responder {
    ///     // ...
    /// #   "index"
    /// }
    /// # fn find_post (_:&mut Input) -> &'static str { "a" }
    /// # fn all_posts (_:&mut Input) -> &'static str { "a" }
    /// # fn add_post (_:&mut Input) -> &'static str { "a" }
    ///
    /// let router = Router::builder()
    ///     .mount("/", |m| {
    ///         m.get("/").handle(Handler::new_ready(index));
    ///     })
    ///     .mount("/api/v1/", |m| {
    ///         m.get("/posts/:id").handle(Handler::new_ready(find_post));
    ///         m.get("/posts").handle(Handler::new_ready(all_posts));
    ///         m.post("/posts").handle(Handler::new_ready(add_post));
    ///     })
    ///     .finish();
    /// # assert!(router.is_ok());
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

    fn modify(&mut self, f: impl FnOnce(&mut Self) -> Result<(), failure::Error>) {
        if self.result.is_ok() {
            self.result = f(self);
        }
    }

    /// Creates an instance of `Router` with current configuration.
    pub fn finish(&mut self) -> Result<Router, failure::Error> {
        let Builder {
            endpoints,
            config,
            result,
        } = mem::replace(self, Router::builder());

        result?;

        let config = config.unwrap_or_default();

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
                builder.insert(path.as_ref(), entry.finish()?);
            }

            builder.finish()?
        };

        Ok(Router {
            recognizer: recognizer,
            endpoints: endpoints,
            config: config,
        })
    }
}

/// A proxy object for adding routes with the certain prefix.
#[derive(Debug)]
pub struct Mount<'a> {
    builder: &'a mut Builder,
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
    pub fn handle(self, handler: Handler) {
        let uri = uri::join_all(self.mount.prefix.iter().chain(Some(&self.suffix)));
        let endpoint = Endpoint::new(uri, self.method, handler);
        self.mount.builder.endpoints.push(endpoint);
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn empty() {
        let router = Router::builder().finish().unwrap();
        assert!(router.recognize("/", &Method::GET).is_err());
    }

    #[test]
    fn root_single_method() {
        let router = Router::builder()
            .mount("/", |m| {
                m.get("/").handle(Handler::new_ready(|_| "a"));
            })
            .finish()
            .unwrap();

        assert_matches!(router.recognize("/", &Method::GET), Ok(Recognize::Matched(0, _)));

        assert!(router.recognize("/path/to", &Method::GET).is_err());
        assert!(router.recognize("/", &Method::POST).is_err());
    }

    #[test]
    fn root_multiple_method() {
        let router = Router::builder()
            .mount("/", |m| {
                m.get("/").handle(Handler::new_ready(|_| "a"));
                m.post("/").handle(Handler::new_ready(|_| "b"));
            })
            .finish()
            .unwrap();

        assert_matches!(router.recognize("/", &Method::GET), Ok(Recognize::Matched(0, _)));
        assert_matches!(router.recognize("/", &Method::POST), Ok(Recognize::Matched(1, _)));

        assert!(router.recognize("/", &Method::PUT).is_err());
    }

    #[test]
    fn root_fallback_head() {
        let router = Router::builder()
            .mount("/", |m| {
                m.get("/").handle(Handler::new_ready(|_| "a"));
            })
            .finish()
            .unwrap();

        assert_matches!(router.recognize("/", &Method::HEAD), Ok(Recognize::Matched(0, _)));
    }

    #[test]
    fn root_fallback_head_disabled() {
        let router = Router::builder()
            .mount("/", |m| {
                m.get("/").handle(Handler::new_ready(|_| "a"));
            })
            .fallback_head(false)
            .finish()
            .unwrap();

        assert!(router.recognize("/", &Method::HEAD).is_err());
    }

    #[test]
    fn fallback_options() {
        let router = Router::builder()
            .mount("/path/to", |m| {
                m.get("/foo").handle(Handler::new_ready(|_| "a"));
                m.post("/foo").handle(Handler::new_ready(|_| "b"));
            })
            .fallback_options(true)
            .finish()
            .unwrap();

        // FIXME:
        assert_matches!(
            router.recognize("/path/to/foo", &Method::OPTIONS),
            Ok(Recognize::Options(_))
        );
    }

    #[test]
    fn fallback_options_disabled() {
        let router = Router::builder()
            .mount("/path/to", |m| {
                m.get("/foo").handle(Handler::new_ready(|_| "a"));
                m.post("/foo").handle(Handler::new_ready(|_| "b"));
            })
            .fallback_options(false)
            .finish()
            .unwrap();

        assert!(router.recognize("/path/to/foo", &Method::OPTIONS).is_err());
    }

    #[test]
    fn mount() {
        let router = Router::builder()
            .mount("/", |m| {
                m.get("/foo").handle(Handler::new_ready(|_| "a")); // /foo
                m.get("/bar").handle(Handler::new_ready(|_| "b")); // /bar
            })
            .mount("/baz", |m| {
                m.get("/").handle(Handler::new_ready(|_| "c")); // /baz

                m.mount("/", |m| {
                    m.get("/").handle(Handler::new_ready(|_| "d")); // /baz
                    m.get("/foobar").handle(Handler::new_ready(|_| "e")); // /baz/foobar
                });
            })
            .finish()
            .unwrap();

        assert_matches!(router.recognize("/foo", &Method::GET), Ok(Recognize::Matched(0, _)));
        assert_matches!(router.recognize("/bar", &Method::GET), Ok(Recognize::Matched(1, _)));
        assert_matches!(router.recognize("/baz", &Method::GET), Ok(Recognize::Matched(3, _)));
        assert_matches!(
            router.recognize("/baz/foobar", &Method::GET),
            Ok(Recognize::Matched(4, _))
        );

        assert!(router.recognize("/baz/", &Method::GET).is_err());
    }
}
