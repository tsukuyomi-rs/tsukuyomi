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
