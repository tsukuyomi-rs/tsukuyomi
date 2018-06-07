use failure;
use futures::{future, Future};
use http::StatusCode;
use std::mem;

use context::Context;
use error::Error;
use output::Output;

use super::recognizer::{self, Recognizer};
use super::route::Route;

#[derive(Debug)]
pub(crate) enum RouterState {
    Uninitialized,
    Matched(usize, Vec<(usize, usize)>),
    NotMatched, // TODO: more informational
}

#[derive(Debug)]
pub struct Router {
    recognizer: Recognizer<usize>,
    routes: Vec<Route>,
}

impl Router {
    pub fn builder() -> Builder {
        Builder {
            builder: Recognizer::builder(),
            routes: vec![],
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
            Some((&i, ..)) if cx.request().method() != self.routes[i].method() => {
                Box::new(future::err(method_not_allowed()))
            }

            Some((&i, params)) => {
                cx.route = RouterState::Matched(i, params);
                self.routes[i].handle(cx)
            }

            None => Box::new(future::err(not_found())),
        }
    }
}

#[derive(Debug)]
pub struct Builder {
    builder: recognizer::Builder<usize>,
    routes: Vec<Route>,
}

impl Builder {
    pub fn mount<I>(&mut self, routes: I) -> &mut Builder
    where
        I: IntoIterator<Item = Route>,
    {
        let orig_routes_len = self.routes.len();
        for (i, route) in routes.into_iter().enumerate() {
            self.builder.insert(route.path(), i + orig_routes_len);
            self.routes.push(route);
        }
        self
    }

    pub fn finish(&mut self) -> Result<Router, failure::Error> {
        Ok(Router {
            recognizer: self.builder.finish()?,
            routes: mem::replace(&mut self.routes, vec![]),
        })
    }
}

fn method_not_allowed() -> Error {
    Error::new(
        format_err!("Invalid Method"),
        StatusCode::METHOD_NOT_ALLOWED,
    )
}

fn not_found() -> Error {
    Error::new(format_err!("No Route"), StatusCode::NOT_FOUND)
}
