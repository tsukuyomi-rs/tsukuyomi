//! Utilities for integrating GraphQL endpoints using Juniper.

// #![doc(html_root_url = "https://docs.rs/tsukuyomi-juniper/0.1.0")]
#![deny(missing_docs)]
#![deny(missing_debug_implementations)]
#![deny(unreachable_pub)]
#![warn(unused_extern_crates)]
#![deny(bare_trait_objects)]
#![warn(warnings)]

#[macro_use]
extern crate futures;
extern crate bytes;
#[macro_use]
extern crate failure;
extern crate http;
extern crate juniper;
#[macro_use]
extern crate serde;
#[cfg_attr(test, macro_use)]
extern crate percent_encoding;
extern crate serde_json;
extern crate serde_qs;
extern crate tsukuyomi;

use bytes::Bytes;
use futures::{Async, Future, IntoFuture, Poll};
use http::{header, Response, StatusCode};
use percent_encoding::percent_decode;
use std::fmt;
use std::sync::Arc;

use juniper::{GraphQLType, InputValue, RootNode};

use tsukuyomi::input::body::FromData;
use tsukuyomi::json::Json;
use tsukuyomi::output::{Output, Responder};
use tsukuyomi::rt::blocking::blocking;
use tsukuyomi::{Error, Handler, Input};

#[allow(missing_docs)]
pub trait GraphQLExecutor: private::Sealed {
    type Future: Future<Item = GraphQLResponse, Error = Error>;

    fn execute(&self, request: GraphQLRequest) -> Self::Future;
}

/// The contextual values for executing GraphQL queries.
pub struct GraphQLState<Q, M, Cx>
where
    Q: GraphQLType<Context = Cx>,
    M: GraphQLType<Context = Cx>,
{
    inner: Arc<(RootNode<'static, Q, M>, Cx)>,
}

impl<Q, M, Cx> Clone for GraphQLState<Q, M, Cx>
where
    Q: GraphQLType<Context = Cx>,
    M: GraphQLType<Context = Cx>,
{
    fn clone(&self) -> Self {
        GraphQLState {
            inner: self.inner.clone(),
        }
    }
}

#[cfg_attr(tarpaulin, skip)]
impl<Q, M, Cx> fmt::Debug for GraphQLState<Q, M, Cx>
where
    Q: GraphQLType<Context = Cx>,
    M: GraphQLType<Context = Cx>,
    RootNode<'static, Q, M>: fmt::Debug,
    Cx: fmt::Debug,
{
    fn fmt(&self, f: &mut fmt::Formatter) -> fmt::Result {
        f.debug_struct("GraphQLState").field("inner", &self.inner).finish()
    }
}

impl<Q, M, Cx> GraphQLState<Q, M, Cx>
where
    Q: GraphQLType<Context = Cx>,
    M: GraphQLType<Context = Cx>,
{
    /// Creates a new `GraphQLState` from components.
    pub fn new(root_node: RootNode<'static, Q, M>, context: Cx) -> GraphQLState<Q, M, Cx> {
        GraphQLState {
            inner: Arc::new((root_node, context)),
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

    /// Create a future for processing the execution of a GraphQL request.
    ///
    /// # Note
    /// This method returns a future but it wlll block the current thread during executing a GraphQL request.
    pub fn execute(&self, request: GraphQLRequest) -> ExecuteResult<Q, M, Cx> {
        ExecuteResult {
            state: self.clone(),
            request: request,
        }
    }
}

impl<Q, M, Cx> GraphQLExecutor for GraphQLState<Q, M, Cx>
where
    Q: GraphQLType<Context = Cx>,
    M: GraphQLType<Context = Cx>,
{
    type Future = ExecuteResult<Q, M, Cx>;

    #[inline(always)]
    fn execute(&self, request: GraphQLRequest) -> Self::Future {
        self.execute(request)
    }
}

mod private {
    use super::*;

    pub trait Sealed {}

    impl<Q, M, Cx> Sealed for GraphQLState<Q, M, Cx>
    where
        Q: GraphQLType<Context = Cx>,
        M: GraphQLType<Context = Cx>,
    {
    }
}

#[doc(hidden)]
pub struct ExecuteResult<Q, M, Cx>
where
    Q: GraphQLType<Context = Cx>,
    M: GraphQLType<Context = Cx>,
{
    state: GraphQLState<Q, M, Cx>,
    request: GraphQLRequest,
}

#[cfg_attr(tarpaulin, skip)]
impl<Q, M, Cx> fmt::Debug for ExecuteResult<Q, M, Cx>
where
    Q: GraphQLType<Context = Cx>,
    M: GraphQLType<Context = Cx>,
    RootNode<'static, Q, M>: fmt::Debug,
    Cx: fmt::Debug,
{
    fn fmt(&self, f: &mut fmt::Formatter) -> fmt::Result {
        f.debug_struct("ExecuteResult")
            .field("state", &self.state)
            .field("request", &self.request)
            .finish()
    }
}
impl<Q, M, Cx> Future for ExecuteResult<Q, M, Cx>
where
    Q: GraphQLType<Context = Cx>,
    M: GraphQLType<Context = Cx>,
{
    type Item = GraphQLResponse;
    type Error = Error;

    fn poll(&mut self) -> Poll<Self::Item, Self::Error> {
        use self::GraphQLBatchRequest::*;
        match self.request.0 {
            Single(ref request) => {
                let response = try_ready!(
                    blocking(|| request.execute(self.state.root_node(), self.state.context()))
                        .map_err(Error::internal_server_error)
                );
                GraphQLResponse::from_single(response).map(Async::Ready)
            }
            Batch(ref requests) => {
                let responses = try_ready!(
                    blocking(|| requests
                        .iter()
                        .map(|request| request.execute(self.state.root_node(), self.state.context()))
                        .collect())
                        .map_err(Error::internal_server_error)
                );
                GraphQLResponse::from_batch(responses).map(Async::Ready)
            }
        }
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

        let params: Params = serde_qs::from_str(s).map_err(|e| Error::bad_request(failure::SyncFailure::new(e)))?;

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
                serde_json::from_str(&*decoded).map(Some).map_err(Error::bad_request)?
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
pub fn graphiql_source(url: &str) -> impl Responder {
    GraphiQLSource(juniper::http::graphiql::graphiql_source(url))
}

#[allow(missing_debug_implementations)]
struct GraphiQLSource(String);

impl Responder for GraphiQLSource {
    fn respond_to(self, _: &Input) -> Result<Output, Error> {
        Response::builder()
            .header(header::CONTENT_TYPE, "text/html; charset=utf-8")
            .body(self.0)
            .map(Into::into)
            .map_err(Error::internal_server_error)
    }
}

/// Creates a handler generating the HTML source to show a GraphiQL interface.
pub fn graphiql_endpoint(url: &str) -> Handler {
    let source = Bytes::from(juniper::http::graphiql::graphiql_source(url));
    Handler::new_ready(move |_| {
        Response::builder()
            .header(header::CONTENT_TYPE, "text/html; charset=utf-8")
            .body(source.clone())
            .map_err(Error::internal_server_error)
    })
}

#[allow(missing_docs)]
pub fn get_graphql_handler<Exec>() -> Handler
where
    Exec: GraphQLExecutor + Send + Sync + 'static,
    Exec::Future: Send + 'static,
{
    Handler::new_async(|input| {
        let request = input
            .uri()
            .query()
            .ok_or_else(|| Error::bad_request(format_err!("empty query")))
            .and_then(GraphQLRequest::from_query);
        request.into_future().and_then(|request| {
            Input::with_current(|input| {
                let cx = input.get::<Exec>();
                cx.execute(request)
            })
        })
    })
}

#[allow(missing_docs)]
pub fn post_graphql_handler<Exec>() -> Handler
where
    Exec: GraphQLExecutor + Send + Sync + 'static,
    Exec::Future: Send + 'static,
{
    Handler::new_async(|input| {
        let request = input.body_mut().read_all().convert_to::<GraphQLRequest>();
        request.into_future().and_then(|request| {
            Input::with_current(|input| {
                let cx = input.get::<Exec>();
                cx.execute(request)
            })
        })
    })
}

#[allow(unreachable_pub)]
#[cfg(test)]
mod tests {
    use super::*;

    use http::Response;
    use juniper::http::tests as http_tests;
    use juniper::tests::model::Database;
    use juniper::{EmptyMutation, RootNode};
    use percent_encoding::{utf8_percent_encode, QUERY_ENCODE_SET};
    use std::cell::RefCell;

    use tsukuyomi::local::{Client, LocalServer};
    use tsukuyomi::output::Data;
    use tsukuyomi::App;

    type Schema = RootNode<'static, Database, EmptyMutation<Database>>;

    type Cx = GraphQLState<Database, EmptyMutation<Database>, Database>;

    fn make_tsukuyomi_app() -> tsukuyomi::AppResult<App> {
        let schema = Schema::new(Database::new(), EmptyMutation::<Database>::new());
        let cx = GraphQLState::new(schema, Database::new());
        App::builder()
            .manage(cx)
            .mount("/", |m| {
                m.get("/").handle(get_graphql_handler::<Cx>());
                m.post("/").handle(post_graphql_handler::<Cx>());
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

        let body = response.body().to_utf8().expect("invalid data").into_owned();

        http_tests::TestResponse {
            status_code,
            content_type,
            body: Some(body),
        }
    }
}
