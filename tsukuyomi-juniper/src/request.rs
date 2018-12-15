use {
    crate::{error::GraphQLParseError, Schema},
    futures::{Async, Future},
    http::{Method, Response, StatusCode},
    juniper::InputValue,
    percent_encoding::percent_decode,
    tsukuyomi::{
        error::Error,
        extractor::Extractor,
        input::{body::RequestBody, header::ContentType, Input},
        output::Responder,
    },
};

fn parse_query_request(input: &mut Input<'_>) -> tsukuyomi::Result<GraphQLRequest> {
    let query_str = input
        .request
        .uri()
        .query()
        .ok_or_else(|| GraphQLParseError::MissingQuery)?;
    parse_query_str(query_str).map_err(Into::into)
}

/// Create an `Extractor` that parses the incoming request as GraphQL query.
pub fn request() -> impl Extractor<Output = (GraphQLRequest,), Error = Error> {
    tsukuyomi::extractor::raw(|input| {
        if input.request.method() == Method::GET {
            return futures::future::Either::A(futures::future::result(
                parse_query_request(input).map(|request| (request,)),
            ));
        }

        if input.request.method() == Method::POST {
            #[allow(missing_debug_implementations)]
            enum RequestKind {
                Json,
                GraphQL,
            }

            let kind = match tsukuyomi::input::header::parse::<ContentType>(input) {
                Ok(Some(mime)) if *mime == mime::APPLICATION_JSON => RequestKind::Json,
                Ok(Some(mime)) if *mime == "application/graphql" => RequestKind::GraphQL,
                Ok(Some(..)) => {
                    return futures::future::Either::A(futures::future::err(
                        GraphQLParseError::InvalidMime.into(),
                    ))
                }
                Ok(None) => {
                    return futures::future::Either::A(futures::future::err(
                        GraphQLParseError::MissingMime.into(),
                    ))
                }
                Err(err) => return futures::future::Either::A(futures::future::err(err)),
            };

            let mut read_all = match input.locals.remove(&RequestBody::KEY) {
                Some(body) => body.read_all(),
                None => {
                    return futures::future::Either::A(futures::future::err(
                        tsukuyomi::error::internal_server_error(
                            "the payload has already stolen by another extractor",
                        ),
                    ))
                }
            };
            let future = futures::future::poll_fn(move || match kind {
                RequestKind::Json => {
                    let data = futures::try_ready!(read_all.poll());
                    let request =
                        serde_json::from_slice(&*data).map_err(GraphQLParseError::ParseJson)?;
                    Ok(Async::Ready((request,)))
                }
                RequestKind::GraphQL => {
                    let data = futures::try_ready!(read_all.poll());
                    String::from_utf8(data.to_vec())
                        .map(|query| Async::Ready((GraphQLRequest::single(query, None, None),)))
                        .map_err(|e| GraphQLParseError::DecodeUtf8(e.utf8_error()).into())
                }
            });

            return futures::future::Either::B(future);
        }

        futures::future::Either::A(futures::future::err(
            GraphQLParseError::InvalidRequestMethod.into(),
        ))
    })
}

fn parse_query_str(s: &str) -> Result<GraphQLRequest, GraphQLParseError> {
    #[derive(Debug, serde::Deserialize)]
    struct ParsedQuery {
        query: String,
        operation_name: Option<String>,
        variables: Option<String>,
    }
    let parsed: ParsedQuery =
        serde_urlencoded::from_str(s).map_err(GraphQLParseError::ParseQuery)?;

    let query = percent_decode(parsed.query.as_ref())
        .decode_utf8()
        .map_err(GraphQLParseError::DecodeUtf8)?
        .into_owned();

    let operation_name = parsed.operation_name.map_or(Ok(None), |s| {
        percent_decode(s.as_ref())
            .decode_utf8()
            .map_err(GraphQLParseError::DecodeUtf8)
            .map(|s| s.into_owned())
            .map(Some)
    })?;

    let variables = parsed
        .variables
        .map_or(Ok(None), |s| -> Result<_, GraphQLParseError> {
            let decoded = percent_decode(s.as_ref())
                .decode_utf8()
                .map_err(GraphQLParseError::DecodeUtf8)?;
            let variables = serde_json::from_str(&*decoded)
                .map(Some)
                .map_err(GraphQLParseError::ParseJson)?;
            Ok(variables)
        })?;

    Ok(GraphQLRequest::single(query, operation_name, variables))
}

/// The type representing a GraphQL request from the client.
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

    /// Creates a `Responder` that executes this request using the specified schema and context.
    pub fn execute<S, CtxT>(self, schema: S, context: CtxT) -> GraphQLResponse<S, CtxT>
    where
        S: Schema + Send + 'static,
        CtxT: AsRef<S::Context> + Send + 'static,
    {
        GraphQLResponse {
            request: self,
            schema,
            context,
        }
    }
}

/// The type representing the result from the executing a GraphQL request.
#[derive(Debug)]
pub struct GraphQLResponse<S, CtxT> {
    request: GraphQLRequest,
    schema: S,
    context: CtxT,
}

impl<S, CtxT> Responder for GraphQLResponse<S, CtxT>
where
    S: Schema + Send + 'static,
    CtxT: AsRef<S::Context> + Send + 'static,
{
    type Response = Response<Vec<u8>>;
    type Error = Error;
    type Future = Box<dyn Future<Item = Self::Response, Error = Self::Error> + Send + 'static>;

    fn respond(self, _: &mut Input<'_>) -> Self::Future {
        let Self {
            request,
            schema,
            context,
        } = self;
        Box::new(
            tsukuyomi::rt::spawn_fn(move || -> tsukuyomi::Result<_> {
                use self::GraphQLRequestKind::*;
                match request.0 {
                    Single(request) => {
                        let response = request.execute(schema.as_root_node(), context.as_ref());
                        let status = if response.is_ok() {
                            StatusCode::OK
                        } else {
                            StatusCode::BAD_REQUEST
                        };
                        let body = serde_json::to_vec(&response)
                            .map_err(tsukuyomi::error::internal_server_error)?;
                        Ok(Response::builder()
                            .status(status)
                            .header("content-type", "application/json")
                            .body(body)
                            .expect("should be a valid response"))
                    }
                    Batch(requests) => {
                        let responses: Vec<_> = requests
                            .iter()
                            .map(|request| request.execute(schema.as_root_node(), context.as_ref()))
                            .collect();
                        let status = if responses.iter().all(|response| response.is_ok()) {
                            StatusCode::OK
                        } else {
                            StatusCode::BAD_REQUEST
                        };
                        let body = serde_json::to_vec(&responses)
                            .map_err(tsukuyomi::error::internal_server_error)?;
                        Ok(Response::builder()
                            .status(status)
                            .header("content-type", "application/json")
                            .body(body)
                            .expect("should be a valid response"))
                    }
                }
            })
            .then(|result| {
                result
                    .map_err(tsukuyomi::error::internal_server_error)
                    .and_then(|result| result)
            }),
        )
    }
}
