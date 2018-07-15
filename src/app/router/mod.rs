mod recognizer;

use failure;
use fnv::FnvHashMap;
use http::header::HeaderValue;
use http::{header, Method, Response};
use std::collections::HashSet;

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
pub(crate) struct Recognize {
    pub(crate) endpoint_id: usize,
    pub(crate) params: Vec<(usize, usize)>,
}

#[derive(Debug)]
pub(super) enum RecognizeErrorKind {
    NotFound,
    MethodNotAllowed,
    FallbackOptions { entry_id: usize },
}

#[derive(Debug)]
pub(super) struct Router {
    pub(super) recognizer: Recognizer,
    pub(super) entries: Vec<RouterEntry>,
    pub(super) config: Config,
}

impl Router {
    pub(super) fn recognize(&self, path: &str, method: &Method) -> Result<Recognize, RecognizeErrorKind> {
        let (i, params) = self.recognizer
            .recognize(path)
            .ok_or_else(|| RecognizeErrorKind::NotFound)?;
        let entry = &self.entries[i];

        match entry.routes.get(method) {
            Some(&i) => Ok(Recognize { endpoint_id: i, params }),
            None if self.config.fallback_head && *method == Method::HEAD => match entry.routes.get(&Method::GET) {
                Some(&i) => Ok(Recognize { endpoint_id: i, params }),
                None => Err(RecognizeErrorKind::MethodNotAllowed),
            },
            None if self.config.fallback_options && *method == Method::OPTIONS => {
                Err(RecognizeErrorKind::FallbackOptions { entry_id: i })
            }
            None => Err(RecognizeErrorKind::MethodNotAllowed),
        }
    }

    pub(super) fn entry(&self, id: usize) -> Option<&RouterEntry> {
        self.entries.get(id)
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

    pub(super) fn fallback_options_response(&self) -> Response<()> {
        let mut response = Response::new(());
        response
            .headers_mut()
            .insert(header::ALLOW, self.allowed_methods.clone());
        response
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
