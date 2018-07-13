mod recognizer;

use failure;
use fnv::FnvHashMap;
use http::header::HeaderValue;
use http::Method;
use std::collections::HashSet;

use error::Error;

pub(super) use self::recognizer::Recognizer;

#[derive(Debug)]
pub(super) struct Config {
    pub(super) fallback_head: bool,
    pub(super) fallback_options: bool,
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
pub(super) enum Recognize {
    Matched(usize, Vec<(usize, usize)>),
    Options(HeaderValue),
}

#[derive(Debug)]
pub(super) struct Router {
    pub(super) recognizer: Recognizer,
    pub(super) entries: Vec<RouterEntry>,
    pub(super) config: Config,
}

impl Router {
    pub(super) fn recognize(&self, path: &str, method: &Method) -> Result<Recognize, Error> {
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
}

#[derive(Debug)]
pub(super) struct RouterEntry {
    routes: FnvHashMap<Method, usize>,
    allowed_methods: HeaderValue,
}

impl RouterEntry {
    pub(super) fn builder() -> RouterEntryBuilder {
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
pub(super) struct RouterEntryBuilder {
    routes: Vec<(Method, usize)>,
    methods: HashSet<Method>,
}

impl RouterEntryBuilder {
    pub(super) fn push(&mut self, method: &Method, i: usize) {
        self.routes.push((method.clone(), i));
        self.methods.insert(method.clone());
    }

    pub(super) fn finish(self) -> Result<RouterEntry, failure::Error> {
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
