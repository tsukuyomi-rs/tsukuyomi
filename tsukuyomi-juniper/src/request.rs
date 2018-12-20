use {
    crate::{error::GraphQLParseError, Schema},
    futures::Future,
    http::{Method, Response, StatusCode},
    juniper::InputValue,
    percent_encoding::percent_decode,
    tsukuyomi::{
        error::Error,
        extractor::Extractor,
        future::{Async, Poll, TryFuture},
        input::{
            body::{ReadAll, RequestBody},
            header::ContentType,
            Input,
        },
        responder::Responder,
    },
};

/// Create an `Extractor` that parses the incoming request as GraphQL query.
pub fn request() -> impl Extractor<
    Output = (GraphQLRequest,), //
    Error = Error,
    Extract = impl TryFuture<Ok = (GraphQLRequest,), Error = Error> + Send + 'static,
> {
    #[allow(missing_debug_implementations)]
    #[derive(Copy, Clone)]
    enum RequestKind {
        Json,
        GraphQL,
    }

    #[allow(missing_debug_implementations)]
    enum State {
        Init,
        Receive(ReadAll, RequestKind),
    }

    tsukuyomi::extractor::extract(|| {
        let mut state = State::Init;
        tsukuyomi::future::poll_fn(move |input| loop {
            state = match state {
                State::Init => {
                    if input.request.method() == Method::GET {
                        return parse_query_request(input).map(|request| Async::Ready((request,)));
                    } else if input.request.method() == Method::POST {
                        let kind = match tsukuyomi::input::header::parse::<ContentType>(input) {
                            Ok(Some(mime)) if *mime == mime::APPLICATION_JSON => RequestKind::Json,
                            Ok(Some(mime)) if *mime == "application/graphql" => {
                                RequestKind::GraphQL
                            }
                            Ok(Some(..)) => return Err(GraphQLParseError::InvalidMime.into()),
                            Ok(None) => return Err(GraphQLParseError::MissingMime.into()),
                            Err(err) => return Err(err),
                        };

                        let read_all = match input.locals.remove(&RequestBody::KEY) {
                            Some(body) => body.read_all(),
                            None => {
                                return Err(tsukuyomi::error::internal_server_error(
                                    "the payload has already stolen by another extractor",
                                ))
                            }
                        };
                        State::Receive(read_all, kind)
                    } else {
                        return Err(GraphQLParseError::InvalidRequestMethod.into());
                    }
                }
                State::Receive(ref mut read_all, kind) => {
                    let data = futures::try_ready!(read_all.poll());
                    match kind {
                        RequestKind::Json => {
                            let request = serde_json::from_slice(&*data)
                                .map_err(GraphQLParseError::ParseJson)?;
                            return Ok(Async::Ready((request,)));
                        }
                        RequestKind::GraphQL => {
                            return String::from_utf8(data.to_vec())
                                .map(|query| {
                                    Async::Ready((GraphQLRequest::single(query, None, None),))
                                })
                                .map_err(|e| GraphQLParseError::DecodeUtf8(e.utf8_error()).into())
                        }
                    }
                }
            };
        })
    })
}

fn parse_query_request(input: &mut Input<'_>) -> tsukuyomi::Result<GraphQLRequest> {
    let query_str = input
        .request
        .uri()
        .query()
        .ok_or_else(|| GraphQLParseError::MissingQuery)?;
    parse_query_str(query_str).map_err(Into::into)
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
    type Respond = GraphQLRespond;

    fn respond(self) -> Self::Respond {
        let Self {
            request,
            schema,
            context,
        } = self;
        let handle = tsukuyomi_rt::spawn_fn(move || -> tsukuyomi::Result<_> {
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
        });

        GraphQLRespond { handle }
    }
}

#[doc(hidden)]
#[allow(missing_debug_implementations)]
pub struct GraphQLRespond {
    handle: tsukuyomi_rt::SpawnHandle<
        tsukuyomi::Result<Response<Vec<u8>>>,
        tsukuyomi_rt::BlockingError,
    >,
}

impl TryFuture for GraphQLRespond {
    type Ok = Response<Vec<u8>>;
    type Error = Error;

    #[inline]
    fn poll_ready(&mut self, _: &mut Input<'_>) -> Poll<Self::Ok, Self::Error> {
        futures::try_ready!(self
            .handle
            .poll()
            .map_err(tsukuyomi::error::internal_server_error))
        .map(Into::into)
    }
}
