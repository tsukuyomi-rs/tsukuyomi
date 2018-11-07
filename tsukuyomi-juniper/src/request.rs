use futures::{Async, Future, Poll};
use http::{Method, Response, StatusCode};
use juniper::InputValue;
use percent_encoding::percent_decode;

use tsukuyomi::error::Error;
use tsukuyomi::extractor::{Extract, Extractor, HasExtractor};
use tsukuyomi::input::Input;
use tsukuyomi::output::Responder;

pub fn request() -> GraphQLRequestExtractor {
    GraphQLRequestExtractor { _priv: () }
}

#[derive(Debug)]
pub struct GraphQLRequestExtractor {
    _priv: (),
}

impl Extractor for GraphQLRequestExtractor {
    type Output = (GraphQLRequest,);
    type Error = Error;
    type Future = GraphQLRequestExtractorFuture;

    fn extract(&self, input: &mut Input<'_>) -> Result<Extract<Self>, Self::Error> {
        match *input.method() {
            Method::GET => {
                let query_str = input
                    .uri()
                    .query()
                    .ok_or_else(|| tsukuyomi::error::bad_request("missing query string"))?;
                let request = parse_query_str(query_str)?;
                Ok(Extract::Ready((request,)))
            }
            Method::POST => {
                let kind = match tsukuyomi::input::header::content_type(input)? {
                    Some(mime) if *mime == mime::APPLICATION_JSON => RequestKind::Json,
                    Some(mime) if *mime == "application/graphql" => RequestKind::GraphQL,
                    Some(mime) => {
                        return Err(tsukuyomi::error::bad_request(format!(
                            "invalid content type: {}",
                            mime
                        )))
                    }
                    None => return Err(tsukuyomi::error::bad_request("missing content-type")),
                };
                Ok(Extract::Incomplete(GraphQLRequestExtractorFuture {
                    kind,
                    read_all: input.body_mut().read_all(),
                }))
            }
            _ => Err(tsukuyomi::error::bad_request("invalid method")),
        }
    }
}

fn parse_query_str(s: &str) -> tsukuyomi::error::Result<GraphQLRequest> {
    #[derive(Debug, serde::Deserialize)]
    struct ParsedQuery<'a> {
        query: &'a str,
        operation_name: Option<&'a str>,
        variables: Option<&'a str>,
    }
    let parsed: ParsedQuery<'_> =
        serde_urlencoded::from_str(s).map_err(tsukuyomi::error::bad_request)?;

    let query = percent_decode(parsed.query.as_ref())
        .decode_utf8()
        .map_err(tsukuyomi::error::bad_request)?
        .into_owned();

    let operation_name = parsed.operation_name.map_or(Ok(None), |s| {
        percent_decode(s.as_ref())
            .decode_utf8()
            .map(|s| s.into_owned())
            .map(Some)
            .map_err(tsukuyomi::error::bad_request)
    })?;

    let variables = parsed
        .variables
        .map_or(Ok(None), |s| -> tsukuyomi::error::Result<_> {
            let decoded = percent_decode(s.as_ref())
                .decode_utf8()
                .map_err(tsukuyomi::error::bad_request)?;
            serde_json::from_str(&*decoded)
                .map(Some)
                .map_err(tsukuyomi::error::bad_request)
        })?;

    Ok(GraphQLRequest::single(query, operation_name, variables))
}

enum RequestKind {
    Json,
    GraphQL,
}

#[doc(hidden)]
#[allow(missing_debug_implementations)]
pub struct GraphQLRequestExtractorFuture {
    kind: RequestKind,
    read_all: tsukuyomi::input::body::ReadAll,
}

impl Future for GraphQLRequestExtractorFuture {
    type Item = (GraphQLRequest,);
    type Error = Error;

    #[inline]
    fn poll(&mut self) -> Poll<Self::Item, Self::Error> {
        let data = futures::try_ready!(self.read_all.poll().map_err(Error::critical));
        match self.kind {
            RequestKind::Json => serde_json::from_slice(&*data)
                .map_err(tsukuyomi::error::bad_request)
                .map(|request| Async::Ready((request,))),
            RequestKind::GraphQL => String::from_utf8(data.to_vec())
                .map(|query| Async::Ready((GraphQLRequest::single(query, None, None),)))
                .map_err(tsukuyomi::error::bad_request),
        }
    }
}

#[derive(Debug, serde::Deserialize)]
pub struct GraphQLRequest(GraphQLRequestKind);

#[derive(Debug, serde::Deserialize)]
#[serde(untagged)]
enum GraphQLRequestKind {
    Single(juniper::http::GraphQLRequest),
    Batch(Vec<juniper::http::GraphQLRequest>),
}

impl GraphQLRequest {
    fn single(
        query: String,
        operation_name: Option<String>,
        variables: Option<InputValue>,
    ) -> Self {
        GraphQLRequest(GraphQLRequestKind::Single(
            juniper::http::GraphQLRequest::new(query, operation_name, variables),
        ))
    }

    pub fn execute<S>(&self, schema: &S, context: &S::Context) -> GraphQLResponse
    where
        S: crate::executor::Schema,
    {
        use self::GraphQLRequestKind::*;
        match self.0 {
            Single(ref request) => {
                let response = request.execute(schema.as_root_node(), context);
                GraphQLResponse {
                    is_ok: response.is_ok(),
                    body: serde_json::to_vec(&response),
                }
            }
            Batch(ref requests) => {
                let responses: Vec<_> = requests
                    .iter()
                    .map(|request| request.execute(schema.as_root_node(), context))
                    .collect();
                GraphQLResponse {
                    is_ok: responses.iter().all(|response| response.is_ok()),
                    body: serde_json::to_vec(&responses),
                }
            }
        }
    }
}

impl HasExtractor for GraphQLRequest {
    type Extractor = GraphQLRequestExtractor;

    #[inline]
    fn extractor() -> Self::Extractor {
        request()
    }
}

#[derive(Debug)]
pub struct GraphQLResponse {
    is_ok: bool,
    body: Result<Vec<u8>, serde_json::Error>,
}

impl Responder for GraphQLResponse {
    type Body = Vec<u8>;
    type Error = Error;

    fn respond_to(self, _: &mut Input<'_>) -> Result<Response<Self::Body>, Self::Error> {
        let status = if self.is_ok {
            StatusCode::OK
        } else {
            StatusCode::BAD_REQUEST
        };
        let body = self.body.map_err(tsukuyomi::error::internal_server_error)?;
        Ok(Response::builder()
            .status(status)
            .header("content-type", "application/json")
            .body(body)
            .expect("should be a valid response"))
    }
}
