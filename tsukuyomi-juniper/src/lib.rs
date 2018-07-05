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
extern crate failure;
extern crate http;
extern crate juniper;
#[macro_use]
extern crate serde;
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
}

impl<Q, M, Cx> Clone for GraphQLContext<Q, M, Cx>
where
    Q: GraphQLType<Context = Cx>,
    M: GraphQLType<Context = Cx>,
{
    fn clone(&self) -> Self {
        GraphQLContext {
            inner: self.inner.clone(),
        }
    }
}

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
    pub fn new(root_node: RootNode<'static, Q, M>, context: Cx) -> GraphQLContext<Q, M, Cx> {
        GraphQLContext {
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
                    let response = try_ready!(
                        tokio_threadpool::blocking(|| request.execute(cx.root_node(), cx.context()))
                            .map_err(Error::internal_server_error)
                    );
                    GraphQLResponse::from_single(response).map(Async::Ready)
                }
                Batch(ref requests) => {
                    let responses = try_ready!(
                        tokio_threadpool::blocking(|| requests
                            .iter()
                            .map(|request| request.execute(cx.root_node(), cx.context()))
                            .collect())
                            .map_err(Error::internal_server_error)
                    );
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
