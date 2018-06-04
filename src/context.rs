use http::Request;
use std::cell::RefCell;

use request::RequestBody;

scoped_thread_local!(static CONTEXT: Context);

#[derive(Debug)]
pub struct Context {
    pub(crate) request: Request<()>,
    pub(crate) payload: RefCell<Option<RequestBody>>,
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
}
