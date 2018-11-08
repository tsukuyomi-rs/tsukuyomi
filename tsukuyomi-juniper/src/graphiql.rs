#![allow(missing_docs)]

use bytes::Bytes;
use http::Response;
use juniper::http::graphiql::graphiql_source;

use tsukuyomi::input::Input;
use tsukuyomi::output::Responder;

/// Creates a handler function which returns a GraphiQL source.
pub fn graphiql(
    url: impl AsRef<str> + 'static,
) -> impl Fn() -> GraphiQLSource + Clone + Send + Sync + 'static {
    let source: Bytes = graphiql_source(url.as_ref()).into();
    move || GraphiQLSource {
        source: source.clone(),
    }
}

#[allow(missing_docs)]
#[derive(Debug)]
pub struct GraphiQLSource {
    source: Bytes,
}

impl Responder for GraphiQLSource {
    type Body = Bytes;
    type Error = tsukuyomi::error::Never;

    #[inline]
    fn respond_to(self, _: &mut Input<'_>) -> Result<Response<Self::Body>, Self::Error> {
        Ok(Response::builder()
            .header("content-type", "text/html; charset=utf-8")
            .body(self.source)
            .expect("should be a valid response"))
    }
}
