use failure;
use fnv::FnvHashMap;
use futures::{future, Future};
use http::{Method, StatusCode};
use std::mem;

use context::Context;
use error::Error;
use output::Output;

use super::recognizer::Recognizer;
use super::route::{normalize_uri, Route};

#[derive(Debug)]
pub(crate) enum RouterState {
    Uninitialized,
    Matched(usize, Vec<(usize, usize)>),
    NotMatched, // TODO: more informational
}

#[derive(Debug)]
pub struct Router {
    recognizer: Recognizer<FnvHashMap<Method, usize>>,
    routes: Vec<Route>,
}

impl Router {
    pub fn builder() -> Builder {
        Builder {
            routes: vec![],
            result: Ok(()),
        }
    }

    pub fn get_route(&self, i: usize) -> Option<&Route> {
        self.routes.get(i)
    }

    pub fn handle(&self, cx: &mut Context) -> Box<Future<Item = Output, Error = Error> + Send> {
        // TODO: fallback HEAD
        // TODO: fallback OPTIONS
        cx.route = RouterState::NotMatched;

        match self.recognizer.recognize(cx.request().uri().path()) {
            Some((matched, params)) => match matched.get(cx.request.method()) {
                Some(&i) => {
                    cx.route = RouterState::Matched(i, params);
                    self.routes[i].handle(cx)
                }
                None => Box::new(future::err(method_not_allowed())),
            },
            None => Box::new(future::err(not_found())),
        }
    }
}

#[derive(Debug)]
pub struct Builder {
    routes: Vec<Route>,
    result: Result<(), failure::Error>,
}

impl Builder {
    pub fn add_route(&mut self, base: &str, route: Route) -> &mut Self {
        if self.result.is_ok() {
            self.result = self.add_route_inner(base, route);
        }
        self
    }

    fn add_route_inner(&mut self, base: &str, mut route: Route) -> Result<(), failure::Error> {
        route.base = normalize_uri(base)?;
        route.path = normalize_uri(&route.path)?;
        self.routes.push(route);
        Ok(())
    }

    pub fn finish(&mut self) -> Result<Router, failure::Error> {
        let Builder { routes, result } = mem::replace(self, Router::builder());
        result?;

        let mut res: FnvHashMap<String, FnvHashMap<Method, usize>> = FnvHashMap::with_hasher(Default::default());
        for (i, route) in routes.iter().enumerate() {
            let full_path = route.full_path();
            res.entry(full_path)
                .or_insert_with(Default::default)
                .insert(route.method().clone(), i);
        }

        let recognizer = {
            let mut builder = Recognizer::builder();
            for (path, methods) in res {
                builder.insert(&path, methods);
            }
            builder.finish()?
        };

        Ok(Router {
            recognizer: recognizer,
            routes: routes,
        })
    }
}

fn method_not_allowed() -> Error {
    Error::new(format_err!("Invalid Method"), StatusCode::METHOD_NOT_ALLOWED)
}

fn not_found() -> Error {
    Error::new(format_err!("No Route"), StatusCode::NOT_FOUND)
}
