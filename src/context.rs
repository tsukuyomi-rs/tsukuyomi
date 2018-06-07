use http::Request;
use std::cell::RefCell;
use std::sync::Arc;

use input::RequestBody;
use router::{Route, Router, RouterState};

scoped_thread_local!(static CONTEXT: Context);

#[derive(Debug)]
pub struct Context {
    pub(crate) request: Request<()>,
    pub(crate) payload: RefCell<Option<RequestBody>>,
    pub(crate) route: RouterState,
    pub(crate) router: Arc<Router>,
}

impl Context {
    pub(crate) fn set<R>(&self, f: impl FnOnce() -> R) -> R {
        CONTEXT.set(self, f)
    }

    pub fn with<R>(f: impl FnOnce(&Context) -> R) -> R {
        CONTEXT.with(f)
    }

    pub fn request(&self) -> &Request<()> {
        &self.request
    }

    pub fn body(&self) -> Option<RequestBody> {
        self.payload.borrow_mut().take()
    }

    pub fn route(&self) -> Option<&Route> {
        match self.route {
            RouterState::Matched(i, ..) => self.router.get_route(i),
            _ => None,
        }
    }

    pub fn params(&self) -> Option<Params> {
        match self.route {
            RouterState::Matched(_, ref params) => Some(Params {
                path: self.request().uri().path(),
                params: &params[..],
            }),
            _ => None,
        }
    }
}

#[derive(Debug)]
pub struct Params<'a> {
    path: &'a str,
    params: &'a [(usize, usize)],
}

impl<'a> Params<'a> {
    pub fn is_empty(&self) -> bool {
        self.params.is_empty()
    }

    pub fn len(&self) -> usize {
        self.params.len()
    }

    pub fn get(&self, i: usize) -> Option<&str> {
        self.params.get(i).and_then(|&(s, e)| self.path.get(s..e))
    }

    pub fn iter(&self) -> impl Iterator<Item = &'a str> + 'a {
        let path = self.path;
        self.params.into_iter().map(move |&(s, e)| &path[s..e])
    }
}
