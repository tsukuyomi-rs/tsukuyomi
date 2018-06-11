use cookie::{Cookie, CookieJar};
use failure::Error;
use http::header;
use http::header::HeaderMap;
use std::cell::{Cell, RefCell};

use app::AppState;

#[derive(Debug, Default)]
pub(crate) struct CookieManager {
    jar: RefCell<CookieJar>,
    init: Cell<bool>,
}

impl CookieManager {
    pub fn is_init(&self) -> bool {
        self.init.get()
    }

    pub fn init(&self, h: &HeaderMap) -> Result<(), Error> {
        let mut jar = self.jar.borrow_mut();
        for raw in h.get_all(header::COOKIE) {
            let raw_s = raw.to_str()?;
            for s in raw_s.split(";").map(|s| s.trim()) {
                let cookie = Cookie::parse_encoded(s)?.into_owned();
                jar.add_original(cookie);
            }
        }
        self.init.set(true);
        Ok(())
    }

    pub fn cookies<'a>(&'a self) -> Cookies<'a> {
        Cookies { jar: &self.jar }
    }

    pub fn append_to(&self, h: &mut HeaderMap) {
        if !self.is_init() {
            return;
        }

        for cookie in self.jar.borrow().delta() {
            h.insert(header::SET_COOKIE, cookie.encoded().to_string().parse().unwrap());
        }
    }
}

#[allow(missing_docs)]
pub struct Cookies<'a> {
    jar: &'a RefCell<CookieJar>,
}

#[allow(missing_docs)]
impl<'a> Cookies<'a> {
    pub fn get(&self, name: &str) -> Option<Cookie<'static>> {
        self.jar.borrow().get(name).map(ToOwned::to_owned)
    }

    pub fn get_private(&self, name: &str) -> Option<Cookie<'static>> {
        AppState::with(|state| self.jar.borrow_mut().private(state.secret_key()).get(name))
    }

    pub fn add(&self, cookie: Cookie<'static>) {
        self.jar.borrow_mut().add(cookie)
    }

    pub fn add_private(&self, cookie: Cookie<'static>) {
        AppState::with(|state| self.jar.borrow_mut().private(state.secret_key()).add(cookie))
    }

    pub fn remove(&self, cookie: Cookie<'static>) {
        self.jar.borrow_mut().remove(cookie)
    }

    pub fn remove_private(&self, cookie: Cookie<'static>) {
        AppState::with(|state| self.jar.borrow_mut().private(state.secret_key()).remove(cookie))
    }
}
