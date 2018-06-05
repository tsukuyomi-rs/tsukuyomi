use super::route::Route;
use context::Context;

#[derive(Debug)]
pub struct RouterContext<'a> {
    pub(super) cx: &'a Context,
    pub(super) route: &'a Route,
    pub(super) params: Vec<(usize, usize)>,
}

impl<'a> RouterContext<'a> {
    pub fn route(&self) -> &'a Route {
        self.route
    }

    pub fn params(&self) -> Params {
        Params {
            path: self.cx.request().uri().path(),
            params: &self.params[..],
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
}
