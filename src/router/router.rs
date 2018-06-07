use failure;
use futures::{future, Future};
use http::StatusCode;
use std::mem;

use context::Context;
use error::Error;
use output::Output;

use super::context::RouterContext;
use super::recognizer::{self, Recognizer};
use super::route::Route;

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

    pub fn handle(&self, cx: &Context) -> Box<Future<Item = Output, Error = Error> + Send> {
        // TODO: fallback HEAD
        // TODO: fallback OPTIONS
        match self.recognizer.recognize(cx.request().uri().path()) {
            Some((&i, params)) => {
                let route = &self.routes[i];
                if cx.request().method() != route.method() {
                    return Box::new(future::err(Error::new(
                        format_err!("Invalid Method"),
                        StatusCode::METHOD_NOT_ALLOWED,
                    )));
                }
                let mut rcx = RouterContext {
                    cx: cx,
                    route: route,
                    params: params,
                };
                route.handle(cx, &mut rcx)
            }
            None => Box::new(future::err(Error::new(
                format_err!("No Route"),
                StatusCode::NOT_FOUND,
            ))),
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
