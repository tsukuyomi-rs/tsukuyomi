use {
    bytes::Bytes,
    http::{Request, Response},
    tsukuyomi::output::IntoResponse,
};

/// Creates a handler function which returns a GraphiQL source.
pub fn graphiql_source(url: impl AsRef<str> + 'static) -> impl IntoResponse + Clone {
    GraphiQLSource {
        source: juniper::http::graphiql::graphiql_source(url.as_ref()).into(),
    }
}

#[derive(Debug, Clone)]
struct GraphiQLSource {
    source: Bytes,
}

impl IntoResponse for GraphiQLSource {
    type Body = Bytes;
    type Error = tsukuyomi::core::Never;

    #[inline]
    fn into_response(self, _: &Request<()>) -> Result<Response<Self::Body>, Self::Error> {
        Ok(Response::builder()
            .header("content-type", "text/html; charset=utf-8")
            .body(self.source)
            .expect("should be a valid response"))
    }
}
