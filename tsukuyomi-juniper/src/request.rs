use futures::{Async, Future};
use http::{Method, Response, StatusCode};
use juniper::InputValue;
use percent_encoding::percent_decode;

use tsukuyomi::error::Error;
use tsukuyomi::extractor::{ExtractStatus, Extractor};
use tsukuyomi::input::Input;
use tsukuyomi::output::Responder;

pub fn request() -> impl Extractor<Output = (GraphQLRequest,), Error = Error> {
    tsukuyomi::extractor::raw(|input| {
        if input.method() == Method::GET {
            let query_str = input
                .uri()
                .query()
                .ok_or_else(|| tsukuyomi::error::bad_request("missing query string"))?;
            let request = parse_query_str(query_str)?;
            return Ok(ExtractStatus::Ready((request,)));
        }

        if input.method() == Method::POST {
            #[allow(missing_debug_implementations)]
            enum RequestKind {
                Json,
                GraphQL,
            }

            let kind = match input.content_type()? {
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

            let mut read_all = input.read_all().ok_or_else(|| {
                tsukuyomi::error::internal_server_error(
                    "The payload has already used by another extractor.",
                )
            })?;
            let future = futures::future::poll_fn(move || match kind {
                RequestKind::Json => {
                    let data = futures::try_ready!(read_all.poll().map_err(Error::critical));
                    let request =
                        serde_json::from_slice(&*data).unwrap_or_else(GraphQLRequest::error);
                    Ok(Async::Ready((request,)))
                }
                RequestKind::GraphQL => {
                    let data = futures::try_ready!(read_all.poll().map_err(Error::critical));
                    String::from_utf8(data.to_vec())
                        .map(|query| Async::Ready((GraphQLRequest::single(query, None, None),)))
                        .map_err(tsukuyomi::error::bad_request)
                }
            });

            return Ok(ExtractStatus::Pending(future));
        }

        Err(tsukuyomi::error::bad_request(format!(
            "the method `{}' is not allowed as a GraphQL request",
            input.method()
        )))
    })
}

fn parse_query_str(s: &str) -> tsukuyomi::error::Result<GraphQLRequest> {
    #[derive(Debug, serde::Deserialize)]
    struct ParsedQuery {
        query: String,
        operation_name: Option<String>,
        variables: Option<String>,
    }
    let parsed: ParsedQuery =
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

#[derive(Debug, serde::Deserialize)]
pub struct GraphQLRequest(GraphQLRequestKind);

#[derive(Debug, serde::Deserialize)]
#[serde(untagged)]
enum GraphQLRequestKind {
    #[serde(skip)]
    ParseError(juniper::FieldError),
    Single(juniper::http::GraphQLRequest),
    Batch(Vec<juniper::http::GraphQLRequest>),
}

impl GraphQLRequest {
    fn error<E>(err: E) -> Self
    where
        E: Into<juniper::FieldError>,
    {
        GraphQLRequest(GraphQLRequestKind::ParseError(err.into()))
    }

    fn single(
        query: String,
        operation_name: Option<String>,
        variables: Option<InputValue>,
    ) -> Self {
        GraphQLRequest(GraphQLRequestKind::Single(
            juniper::http::GraphQLRequest::new(query, operation_name, variables),
        ))
    }

    pub fn execute<S>(self, schema: &S, context: &S::Context) -> GraphQLResponse
    where
        S: crate::executor::Schema,
    {
        use self::GraphQLRequestKind::*;
        match self.0 {
            ParseError(err) => GraphQLResponse {
                is_ok: false,
                body: serde_json::to_vec(&juniper::http::GraphQLResponse::error(err)),
            },
            Single(request) => {
                let response = request.execute(schema.as_root_node(), context);
                GraphQLResponse {
                    is_ok: response.is_ok(),
                    body: serde_json::to_vec(&response),
                }
            }
            Batch(requests) => {
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
