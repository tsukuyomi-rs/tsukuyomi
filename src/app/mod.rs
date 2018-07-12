//! The definition of components for constructing the HTTP applications.

#![allow(missing_docs)]

pub mod builder;
pub mod service;

mod endpoint;
mod recognizer;
mod uri;

#[cfg(test)]
mod tests;

use failure;
use fnv::FnvHashMap;
use http::header::HeaderValue;
use http::{header, Method, Response};
use state::Container;
use std::collections::HashSet;
use std::fmt;
use std::sync::Arc;

use error::handler::ErrorHandler;
use error::Error;
use handler::Handle;
use input::Input;
use modifier::Modifier;

pub use self::builder::AppBuilder;
pub use self::endpoint::Endpoint;
use self::recognizer::Recognizer;
pub use self::uri::Uri;

#[derive(Debug)]
pub struct Config {
    pub fallback_head: bool,
    pub fallback_options: bool,
    _priv: (),
}

impl Default for Config {
    fn default() -> Self {
        Config {
            fallback_head: true,
            fallback_options: false,
            _priv: (),
        }
    }
}

#[derive(Debug)]
enum Recognize {
    Matched(usize, Vec<(usize, usize)>),
    Options(HeaderValue),
}

/// The global and shared variables used throughout the serving an HTTP application.
pub struct AppState {
    recognizer: Recognizer,
    entries: Vec<RouterEntry>,
    endpoints: Vec<Endpoint>,
    config: Config,

    error_handler: Box<dyn ErrorHandler + Send + Sync + 'static>,
    modifiers: Vec<Box<dyn Modifier + Send + Sync + 'static>>,
    states: Container,
}

#[cfg_attr(tarpaulin, skip)]
impl fmt::Debug for AppState {
    fn fmt(&self, f: &mut fmt::Formatter) -> fmt::Result {
        f.debug_struct("AppState").finish()
    }
}

impl AppState {
    /// Gets the reference to i-th `Route` contained in this router.
    pub fn get(&self, i: usize) -> Option<&Endpoint> {
        self.endpoints.get(i)
    }

    fn recognize(&self, path: &str, method: &Method) -> Result<Recognize, Error> {
        let (i, params) = self.recognizer.recognize(path).ok_or_else(|| Error::not_found())?;
        let entry = &self.entries[i];

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

    /// Returns the reference to `ErrorHandler` contained in this value.
    pub fn error_handler(&self) -> &dyn ErrorHandler {
        &*self.error_handler
    }

    /// Returns the reference to `ErrorHandler` contained in this value.
    pub fn modifiers(&self) -> &[Box<dyn Modifier + Send + Sync + 'static>] {
        &self.modifiers
    }

    pub fn states(&self) -> &Container {
        &self.states
    }
}

/// The main type which represents an HTTP application.
#[derive(Debug)]
pub struct App {
    global: Arc<AppState>,
}

impl App {
    /// Creates a builder object for constructing an instance of this type.
    pub fn builder() -> AppBuilder {
        AppBuilder::new()
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
