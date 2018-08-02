use cookie::{Cookie, CookieJar};
use failure::Error;
use http::header::{self, HeaderMap};

#[derive(Debug)]
pub(crate) struct CookieManager {
    pub(super) jar: CookieJar,
    is_init: bool,
}

impl CookieManager {
    pub(crate) fn new() -> CookieManager {
        CookieManager {
            jar: CookieJar::new(),
            is_init: false,
        }
    }

    pub(crate) fn is_init(&self) -> bool {
        self.is_init
    }

    pub(crate) fn init(&mut self, h: &HeaderMap) -> Result<(), Error> {
        for raw in h.get_all(header::COOKIE) {
            let raw_s = raw.to_str()?;
            for s in raw_s.split(';').map(|s| s.trim()) {
                let cookie = Cookie::parse_encoded(s)?.into_owned();
                self.jar.add_original(cookie);
            }
        }

        self.is_init = true;

        Ok(())
    }

    pub(crate) fn append_to(&self, h: &mut HeaderMap) {
        if !self.is_init {
            return;
        }

        for cookie in self.jar.delta() {
            h.insert(
                header::SET_COOKIE,
                cookie.encoded().to_string().parse().unwrap(),
            );
        }
    }
}
