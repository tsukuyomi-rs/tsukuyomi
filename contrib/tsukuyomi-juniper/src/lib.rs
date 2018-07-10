//! Utilities to process GraphQL requests in Tsukuyomi, by using Juniper.
//!
//! # Examples
//!
//! ```
//! # extern crate tsukuyomi;
//! # extern crate tsukuyomi_juniper;
//! # #[macro_use]
//! # extern crate juniper;
//! # use tsukuyomi::App;
//! use tsukuyomi_juniper::{GraphQLState, AppGraphQLExt};
//!
//! struct Context {/* ... */}
//! impl juniper::Context for Context {}
//!
//! struct Query {/* ... */}
//! graphql_object!(Query: Context |&self| {/* ... */});
//!
//! struct Mutation {/* ... */}
//! graphql_object!(Mutation: Context |&self| {/* ... */});
//!
//! # fn main() -> tsukuyomi::AppResult<()> {
//! let context = Context {/* ... */};
//! let schema = juniper::RootNode::new(Query {/*...*/}, Mutation {/*...*/});
//!
//! let state = GraphQLState::new(context, schema);
//!
//! let app = App::builder()
//!     .graphql("/graphql", state)
//!     .graphiql("/graphiql", "http://localhost:4000/graphql")
//!     .finish()?;
//! # Ok(())
//! # }
//! ```

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
extern crate hyperx;
extern crate juniper;
extern crate mime;
#[macro_use]
extern crate serde;
#[cfg_attr(test, macro_use)]
extern crate percent_encoding;
extern crate serde_json;
extern crate serde_qs;
extern crate tsukuyomi;

use bytes::Bytes;
use futures::{Async, Future, Poll};
use http::{header, Response, StatusCode};
use hyperx::header::ContentType;
use percent_encoding::percent_decode;
use std::fmt;
use std::sync::Arc;

use juniper::{GraphQLType, InputValue, RootNode};

use tsukuyomi::app::AppBuilder;
use tsukuyomi::input::body::FromData;
use tsukuyomi::output::{Output, Responder};
use tsukuyomi::rt::blocking::blocking;
use tsukuyomi::{Error, Input};

/// Abstraction of an executor which processes asynchronously the GraphQL requests.
pub trait GraphQLExecutor {
    /// The type of future which will be returned from `execute`.
    type Future: Future<Item = GraphQLResponse, Error = Error>;

    /// Creates a future to process an execution of a GraphQL request.
    fn execute(&self, request: GraphQLRequest) -> Self::Future;
}

/// The main type containing all contextual values for processing GraphQL requests.
#[derive(Debug)]
pub struct GraphQLState<T, QueryT, MutationT>
where
    QueryT: GraphQLType<Context = T>,
    MutationT: GraphQLType<Context = T>,
{
    inner: Arc<GraphQLStateInner<T, QueryT, MutationT>>,
}

struct GraphQLStateInner<T, QueryT, MutationT>
where
    QueryT: GraphQLType<Context = T>,
    MutationT: GraphQLType<Context = T>,
{
    context: T,
    root_node: RootNode<'static, QueryT, MutationT>,
}

#[cfg_attr(tarpaulin, skip)]
impl<T, QueryT, MutationT> fmt::Debug for GraphQLStateInner<T, QueryT, MutationT>
where
    QueryT: GraphQLType<Context = T>,
    MutationT: GraphQLType<Context = T>,
    T: fmt::Debug,
{
    fn fmt(&self, f: &mut fmt::Formatter) -> fmt::Result {
        f.debug_struct("GraphQLStateInner")
            .field("context", &self.context)
            .finish()
    }
}

impl<T, QueryT, MutationT> Clone for GraphQLState<T, QueryT, MutationT>
where
    QueryT: GraphQLType<Context = T>,
    MutationT: GraphQLType<Context = T>,
{
    fn clone(&self) -> Self {
        GraphQLState {
            inner: self.inner.clone(),
        }
    }
}

impl<T, QueryT, MutationT> GraphQLState<T, QueryT, MutationT>
where
    QueryT: GraphQLType<Context = T>,
    MutationT: GraphQLType<Context = T>,
{
    /// Creates a new `GraphQLState` from components.
    pub fn new(context: T, root_node: RootNode<'static, QueryT, MutationT>) -> GraphQLState<T, QueryT, MutationT> {
        GraphQLState {
            inner: Arc::new(GraphQLStateInner { context, root_node }),
        }
    }

    /// Returns the reference to context in this value.
    pub fn context(&self) -> &T {
        &self.inner.context
    }

    /// Returns the reference to root node in this value.
    pub fn root_node(&self) -> &RootNode<'static, QueryT, MutationT> {
        &self.inner.root_node
    }

    /// Create a future for processing the execution of a GraphQL request.
    pub fn execute(&self, request: GraphQLRequest) -> Execute<T, QueryT, MutationT> {
        Execute {
            state: self.clone(),
            request,
        }
    }
}

/// A `Future` representing a process to execute a GraphQL request from peer.
#[derive(Debug)]
pub struct Execute<T, QueryT, MutationT>
where
    QueryT: GraphQLType<Context = T>,
    MutationT: GraphQLType<Context = T>,
{
    state: GraphQLState<T, QueryT, MutationT>,
    request: GraphQLRequest,
}

impl<T, QueryT, MutationT> Future for Execute<T, QueryT, MutationT>
where
    QueryT: GraphQLType<Context = T>,
    MutationT: GraphQLType<Context = T>,
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

impl<T, QueryT, MutationT> GraphQLExecutor for GraphQLState<T, QueryT, MutationT>
where
    QueryT: GraphQLType<Context = T>,
    MutationT: GraphQLType<Context = T>,
{
    type Future = Execute<T, QueryT, MutationT>;

    #[inline(always)]
    fn execute(&self, request: GraphQLRequest) -> Self::Future {
        self.execute(request)
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

impl GraphQLRequest {
    /// Creates a single GraphQL request.
    pub fn single(request: juniper::http::GraphQLRequest) -> GraphQLRequest {
        GraphQLRequest(GraphQLBatchRequest::Single(request))
    }

    /// Creates a batch GraphQL requests.
    pub fn batches<I>(iter: I) -> GraphQLRequest
    where
        I: IntoIterator<Item = juniper::http::GraphQLRequest>,
    {
        GraphQLRequest(GraphQLBatchRequest::Batch(iter.into_iter().collect()))
    }

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

    /// Parses a query string into a set of GraphQL request.
    ///
    /// The provided payload must be a valid JSON data.
    pub fn from_payload(payload: &[u8]) -> Result<GraphQLRequest, Error> {
        serde_json::from_slice(payload)
            .map_err(Error::bad_request)
            .map(GraphQLRequest)
    }

    /// Returns `true` if this request is a batch request.
    pub fn is_batch(&self) -> bool {
        match self.0 {
            GraphQLBatchRequest::Batch(..) => true,
            _ => false,
        }
    }
}

impl FromData for GraphQLRequest {
    fn from_data(data: Bytes, input: &Input) -> Result<Self, Error> {
        if let Some(ContentType(mime)) = input.header()? {
            if mime != mime::APPLICATION_JSON {
                return Err(Error::bad_request(format_err!(
                    "The value of Content-type is not equal to application/json"
                )));
            }
        }

        Self::from_payload(&*data)
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

/// Definitions of `Handler`s for serving GraphQL requests.
pub mod endpoint {
    use bytes::Bytes;
    use futures::{Future, IntoFuture};
    use http::{header, Response};
    use juniper;

    use tsukuyomi::error::Error;
    use tsukuyomi::handler::Handler;
    use tsukuyomi::input::Input;
    use tsukuyomi::output::{Output, Responder};

    use super::{GraphQLExecutor, GraphQLRequest};

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
    pub fn graphiql(url: &str) -> Handler {
        let source = Bytes::from(juniper::http::graphiql::graphiql_source(url));
        Handler::new_ready(move |_| {
            Response::builder()
                .header(header::CONTENT_TYPE, "text/html; charset=utf-8")
                .body(source.clone())
                .map_err(Error::internal_server_error)
        })
    }

    /// Creates a handler processing a GraphQL request passed as an HTTP GET request.
    ///
    /// The GraphQL query is represented as HTTP query string.
    /// See [the documentation of GraphQL][get-request] for details.
    ///
    /// [get-request]: https://graphql.org/learn/serving-over-http/#get-request
    pub fn graphql_get<Exec>() -> Handler
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

    /// Creates a handler processing a GraphQL request passed as an HTTP POST request.
    ///
    /// The GraphQL request is represented as a JSON payload.
    /// See [the documentation of GraphQL][post-request] for details.
    ///
    /// [post-request]: https://graphql.org/learn/serving-over-http/#post-request
    pub fn graphql_post<Exec>() -> Handler
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
}

/// A set of extensions for registration of GraphQL endpoints into `App`.
pub trait AppGraphQLExt: sealed::Sealed {
    /// Registers a set of GraphQL handlers mounted to the provided path.
    ///
    /// The following endpoints will be registered:
    ///
    /// * `GET <path>`
    /// * `POST <path>`
    fn graphql<T>(&mut self, path: &str, executor: T) -> &mut Self
    where
        T: GraphQLExecutor + Send + Sync + 'static,
        T::Future: Send + 'static;

    /// Registers a GraphiQL endpoint mounted to the provided path into this application.
    fn graphiql(&mut self, path: &str, url: &str) -> &mut Self;
}

impl AppGraphQLExt for AppBuilder {
    fn graphql<T>(&mut self, path: &str, executor: T) -> &mut Self
    where
        T: GraphQLExecutor + Send + Sync + 'static,
        T::Future: Send + 'static,
    {
        self.manage(executor).mount(path, |m| {
            m.get("/").handle(endpoint::graphql_get::<T>());
            m.post("/").handle(endpoint::graphql_post::<T>());
        })
    }

    fn graphiql(&mut self, path: &str, url: &str) -> &mut Self {
        self.mount(path, |m| {
            m.get("/").handle(endpoint::graphiql(url));
        })
    }
}

// TODO: impl for Mount

mod sealed {
    use tsukuyomi::app::AppBuilder;

    pub trait Sealed {}

    impl Sealed for AppBuilder {}
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

    fn make_tsukuyomi_app() -> tsukuyomi::AppResult<App> {
        let context = Database::new();
        let schema = Schema::new(Database::new(), EmptyMutation::<Database>::new());
        let state = GraphQLState::new(context, schema);

        App::builder().graphql("/", state).finish()
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
