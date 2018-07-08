#[cfg_attr(feature = "tera", macro_use)]
extern crate failure;
extern crate http;
extern crate tsukuyomi;

#[cfg(feature = "askama")]
pub mod askama {
    extern crate askama;

    use failure::SyncFailure;
    use http::{header, Response};
    use tsukuyomi::output::{Output, Responder};
    use tsukuyomi::{Error, Input};

    #[derive(Debug)]
    pub struct Template<T: self::askama::Template>(pub T);

    impl<T> Responder for Template<T>
    where
        T: self::askama::Template,
    {
        fn respond_to(self, _: &Input) -> Result<Output, Error> {
            let body = self.0
                .render()
                .map_err(|e| Error::internal_server_error(SyncFailure::new(e)))?;

            Response::builder()
                .header(header::CONTENT_TYPE, "text/html; charset=utf-8")
                .body(body)
                .map(Into::into)
                .map_err(Error::internal_server_error)
        }
    }
}

#[cfg(feature = "tera")]
pub mod tera {
    extern crate serde;
    extern crate tera;

    use failure::SyncFailure;
    use http::{header, Response};
    use std::borrow::Cow;
    use tsukuyomi::output::{Output, Responder};
    use tsukuyomi::{Error, Input};

    use self::serde::ser::Serialize;
    use self::tera::Tera;

    #[derive(Debug)]
    pub struct Template<T: Serialize> {
        name: Cow<'static, str>,
        data: T,
    }

    impl<T> Template<T>
    where
        T: Serialize,
    {
        pub fn new<S>(name: S, data: T) -> Template<T>
        where
            S: Into<Cow<'static, str>>,
        {
            Template {
                name: name.into(),
                data: data,
            }
        }
    }

    impl<T> Responder for Template<T>
    where
        T: Serialize,
    {
        fn respond_to(self, input: &Input) -> Result<Output, Error> {
            let tera = input
                .try_get::<Tera>()
                .ok_or_else(|| Error::internal_server_error(format_err!("Tera template engine is not set.")))?;

            let body = tera.render(&self.name, &self.data)
                .map_err(|e| Error::internal_server_error(SyncFailure::new(e)))?;

            Response::builder()
                .header(header::CONTENT_TYPE, "text/html; charset=utf-8")
                .body(body)
                .map(Into::into)
                .map_err(Error::internal_server_error)
        }
    }
}
