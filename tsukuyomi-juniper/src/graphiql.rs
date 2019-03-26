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
    #[inline]
    fn into_response(self, _: &Request<()>) -> tsukuyomi::Result<tsukuyomi::output::Response> {
        Ok(Response::builder()
            .header("content-type", "text/html; charset=utf-8")
            .body(self.source.into())
            .expect("should be a valid response"))
    }
}
