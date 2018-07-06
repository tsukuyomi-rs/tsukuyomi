//! Utilities for integrating GraphQL endpoints using Juniper.

// #![doc(html_root_url = "https://docs.rs/tsukuyomi-juniper/0.1.0")]
#![deny(missing_docs)]
#![deny(missing_debug_implementations)]
#![deny(unreachable_pub)]
#![deny(unused_extern_crates)]
#![deny(warnings)]
#![deny(bare_trait_objects)]

#[macro_use]
extern crate futures;
extern crate bytes;
#[cfg_attr(test, macro_use)]
extern crate failure;
extern crate http;
extern crate juniper;
#[macro_use]
extern crate serde;
#[cfg_attr(test, macro_use)]
extern crate percent_encoding;
extern crate serde_json;
extern crate serde_qs;
extern crate tokio_threadpool;
extern crate tsukuyomi;

use bytes::Bytes;
use futures::future::poll_fn;
use futures::{Async, Future};
use http::{header, Response, StatusCode};
use percent_encoding::percent_decode;
use std::fmt;
use std::sync::Arc;

use juniper::{GraphQLType, InputValue, RootNode};

use tsukuyomi::input::body::FromData;
use tsukuyomi::json::Json;
use tsukuyomi::output::{Output, Responder};
use tsukuyomi::{Error, Input};

/// The contextual values for executing GraphQL queries.
pub struct GraphQLContext<Q, M, Cx>
where
    Q: GraphQLType<Context = Cx>,
    M: GraphQLType<Context = Cx>,
{
    inner: Arc<(RootNode<'static, Q, M>, Cx)>,
    should_transit: bool,
}

impl<Q, M, Cx> Clone for GraphQLContext<Q, M, Cx>
where
    Q: GraphQLType<Context = Cx>,
    M: GraphQLType<Context = Cx>,
{
    fn clone(&self) -> Self {
        GraphQLContext {
            inner: self.inner.clone(),
            should_transit: self.should_transit,
        }
    }
}

#[cfg_attr(tarpaulin, skip)]
impl<Q, M, Cx> fmt::Debug for GraphQLContext<Q, M, Cx>
where
    Q: GraphQLType<Context = Cx>,
    M: GraphQLType<Context = Cx>,
    RootNode<'static, Q, M>: fmt::Debug,
    Cx: fmt::Debug,
{
    fn fmt(&self, f: &mut fmt::Formatter) -> fmt::Result {
        f.debug_struct("GraphQLContext")
            .field("inner", &self.inner)
            .finish()
    }
}

impl<Q, M, Cx> GraphQLContext<Q, M, Cx>
where
    Q: GraphQLType<Context = Cx>,
    M: GraphQLType<Context = Cx>,
{
    /// Creates a new `GraphQLContext` from components.
    pub fn new(
        root_node: RootNode<'static, Q, M>,
        context: Cx,
        should_transit: bool,
    ) -> GraphQLContext<Q, M, Cx> {
        GraphQLContext {
            inner: Arc::new((root_node, context)),
            should_transit: should_transit,
        }
    }

    /// Returns the reference to root node in this value.
    pub fn root_node(&self) -> &RootNode<'static, Q, M> {
        &self.inner.0
    }

    /// Returns the reference to context in this value.
    pub fn context(&self) -> &Cx {
        &self.inner.1
    }

    #[allow(missing_docs)]
    pub fn should_transit(&self) -> bool {
        self.should_transit
    }

    /// Executes an incoming GraphQL query with this context.
    ///
    /// # Note
    /// This method returns a Future, but it is possible to block the curren thread during
    /// polling the result (see the documentation of `tokio_threadpool::blocking` for details.)
    pub fn execute(
        &self,
        request: GraphQLRequest,
    ) -> impl Future<Item = GraphQLResponse, Error = Error> {
        let cx = self.clone();
        poll_fn(move || {
            use self::GraphQLBatchRequest::*;
            match request.0 {
                Single(ref request) => {
                    let response = if cx.should_transit {
                        try_ready!(
                            tokio_threadpool::blocking(
                                || request.execute(cx.root_node(), cx.context())
                            ).map_err(Error::internal_server_error)
                        )
                    } else {
                        request.execute(cx.root_node(), cx.context())
                    };
                    GraphQLResponse::from_single(response).map(Async::Ready)
                }
                Batch(ref requests) => {
                    let responses = if cx.should_transit {
                        try_ready!(
                            tokio_threadpool::blocking(|| requests
                                .iter()
                                .map(|request| request.execute(cx.root_node(), cx.context()))
                                .collect())
                                .map_err(Error::internal_server_error)
                        )
                    } else {
                        requests
                            .iter()
                            .map(|request| request.execute(cx.root_node(), cx.context()))
                            .collect()
                    };
                    GraphQLResponse::from_batch(responses).map(Async::Ready)
                }
            }
        })
    }
}

/// A wrapper around an incoming GraphQL request from a client.
#[derive(Debug)]
pub struct GraphQLRequest(GraphQLBatchRequest);

#[derive(Debug, Deserialize)]
#[serde(untagged)]
enum GraphQLBatchRequest {
    Single(juniper::http::GraphQLRequest),
    Batch(Vec<juniper::http::GraphQLRequest>),
}

impl FromData for GraphQLRequest {
    fn from_data(data: Bytes, input: &Input) -> Result<Self, Error> {
        FromData::from_data(data, input).map(|Json(request)| GraphQLRequest(request))
    }
}

impl GraphQLRequest {
    /// Parses a query string into a single GraphQL request.
    pub fn from_query(s: &str) -> Result<GraphQLRequest, Error> {
        #[derive(Debug, Deserialize)]
        struct Params {
            query: String,
            operation_name: Option<String>,
            variables: Option<String>,
        }

        let params: Params =
            serde_qs::from_str(s).map_err(|e| Error::bad_request(failure::SyncFailure::new(e)))?;

        let query = percent_decode(params.query.as_bytes())
            .decode_utf8()
            .map_err(Error::bad_request)?
            .into_owned();

        let operation_name = match params.operation_name {
            Some(s) => Some(
                percent_decode(s.as_bytes())
                    .decode_utf8()
                    .map_err(Error::bad_request)?
                    .into_owned(),
            ),
            None => None,
        };

        let variables: Option<InputValue> = match params.variables {
            Some(variables) => {
                let decoded = percent_decode(variables.as_bytes())
                    .decode_utf8()
                    .map_err(Error::bad_request)?;
                serde_json::from_str(&*decoded)
                    .map(Some)
                    .map_err(Error::bad_request)?
            }
            None => None,
        };

        let request = juniper::http::GraphQLRequest::new(query, operation_name, variables);

        Ok(GraphQLRequest(GraphQLBatchRequest::Single(request)))
    }
}

/// The result of executing a GraphQL query.
#[derive(Debug)]
pub struct GraphQLResponse {
    status: StatusCode,
    body: String,
}

impl GraphQLResponse {
    fn from_single(response: juniper::http::GraphQLResponse) -> Result<Self, Error> {
        let status = if response.is_ok() {
            StatusCode::OK
        } else {
            StatusCode::BAD_REQUEST
        };
        serde_json::to_string(&response)
            .map(|body| GraphQLResponse { status, body })
            .map_err(Error::internal_server_error)
    }

    fn from_batch(responses: Vec<juniper::http::GraphQLResponse>) -> Result<Self, Error> {
        let status = if responses.iter().all(|response| response.is_ok()) {
            StatusCode::OK
        } else {
            StatusCode::BAD_REQUEST
        };
        serde_json::to_string(&responses)
            .map(|body| GraphQLResponse { status, body })
            .map_err(Error::internal_server_error)
    }

    #[allow(missing_docs)]
    pub fn custom(status: StatusCode, body: serde_json::Value) -> GraphQLResponse {
        GraphQLResponse {
            status,
            body: body.to_string(),
        }
    }
}

impl Responder for GraphQLResponse {
    fn respond_to(self, _input: &Input) -> Result<Output, Error> {
        Response::builder()
            .status(self.status)
            .header(header::CONTENT_TYPE, "application/json")
            .body(self.body)
            .map(Into::into)
            .map_err(Error::internal_server_error)
    }
}

/// Generates the HTML source to show a GraphiQL interface.
pub fn graphiql_source(url: &str) -> GraphiQLSource {
    GraphiQLSource(juniper::http::graphiql::graphiql_source(url))
}

/// A `Responder` representing the HTML source of GraphiQL interface.
#[derive(Debug)]
pub struct GraphiQLSource(String);

impl Responder for GraphiQLSource {
    fn respond_to(self, _: &Input) -> Result<Output, Error> {
        Response::builder()
            .header(header::CONTENT_TYPE, "text/html; charset=utf-8")
            .body(self.0)
            .map(Into::into)
            .map_err(Error::internal_server_error)
    }
}

#[allow(unreachable_pub)]
#[cfg(test)]
mod tests {
    use super::*;

    use futures::{Future, IntoFuture};
    use http::Response;
    use juniper::http::tests as http_tests;
    use juniper::tests::model::Database;
    use juniper::{EmptyMutation, RootNode};
    use percent_encoding::{utf8_percent_encode, QUERY_ENCODE_SET};
    use std::cell::RefCell;

    use tsukuyomi::local::{Client, LocalServer};
    use tsukuyomi::output::Data;
    use tsukuyomi::{App, Handler};

    type Schema = RootNode<'static, Database, EmptyMutation<Database>>;

    type Cx = GraphQLContext<Database, EmptyMutation<Database>, Database>;

    fn get_graphql(input: &mut Input) -> impl Future<Item = GraphQLResponse, Error = Error> {
        let request = input
            .uri()
            .query()
            .ok_or_else(|| Error::bad_request(format_err!("empty query")))
            .and_then(GraphQLRequest::from_query);
        request.into_future().and_then(|request| {
            Input::with_current(|input| {
                let cx = input.get::<Cx>();
                cx.execute(request)
            })
        })
    }

    fn post_graphql(input: &mut Input) -> impl Future<Item = GraphQLResponse, Error = Error> {
        let request = input.body_mut().read_all().convert_to::<GraphQLRequest>();
        request.into_future().and_then(|request| {
            Input::with_current(|input| {
                let cx = input.get::<Cx>();
                cx.execute(request)
            })
        })
    }

    fn make_tsukuyomi_app() -> tsukuyomi::AppResult<App> {
        let schema = Schema::new(Database::new(), EmptyMutation::<Database>::new());
        let cx = GraphQLContext::new(schema, Database::new(), false);
        App::builder()
            .manage(cx)
            .mount("/", |m| {
                m.get("/").handle(Handler::new_async(get_graphql));
                m.post("/").handle(Handler::new_async(post_graphql));
            })
            .finish()
    }

    struct TestTsukuyomiIntegration<'a> {
        client: RefCell<Client<'a>>,
    }

    define_encode_set!{
        pub DUMMY_ENCODE_SET = [QUERY_ENCODE_SET] | {'{', '}'}
    }

    fn encoded_url(url: &str) -> String {
        utf8_percent_encode(url, DUMMY_ENCODE_SET).to_string()
    }

    impl<'a> http_tests::HTTPIntegration for TestTsukuyomiIntegration<'a> {
        fn get(&self, url: &str) -> http_tests::TestResponse {
            let response = self.client
                .borrow_mut()
                .get(encoded_url(url))
                .execute()
                .expect("unexpected error during handling a request");
            make_test_response(response)
        }

        fn post(&self, url: &str, body: &str) -> http_tests::TestResponse {
            let response = self.client
                .borrow_mut()
                .post(encoded_url(url))
                .header(header::CONTENT_TYPE, "application/json")
                .body(body.to_owned())
                .execute()
                .expect("unexpected error during handling a request");
            make_test_response(response)
        }
    }

    #[test]
    fn test_tsukuyomi_integration() {
        let app = make_tsukuyomi_app().expect("failed to create an App.");
        let mut server = LocalServer::new(app).expect("failed to create LocalServer");
        let integration = TestTsukuyomiIntegration {
            client: RefCell::new(server.client()),
        };

        http_tests::run_http_test_suite(&integration);
    }

    fn make_test_response(response: Response<Data>) -> http_tests::TestResponse {
        let status_code = response.status().as_u16() as i32;

        let content_type = response
            .headers()
            .get(header::CONTENT_TYPE)
            .expect("missing Content-Type")
            .to_str()
            .expect("invalid content-type")
            .to_owned();

        let body = response
            .body()
            .to_utf8()
            .expect("invalid data")
            .into_owned();

        http_tests::TestResponse {
            status_code,
            content_type,
            body: Some(body),
        }
    }
}
