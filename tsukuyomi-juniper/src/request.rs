use {
    crate::{error::GraphQLParseError, Schema},
    futures::{stream::Concat2, Future, Stream},
    http::{Method, Response, StatusCode},
    juniper::{DefaultScalarValue, InputValue, ScalarRefValue, ScalarValue},
    percent_encoding::percent_decode,
    serde::Deserialize,
    tsukuyomi::{
        error::Error,
        extractor::Extractor,
        future::{Async, Poll, TryFuture},
        input::{body::RequestBody, header::ContentType, localmap::LocalData, Input},
        responder::Responder,
    },
};

/// Create an `Extractor` that parses the incoming request as GraphQL query.
pub fn request<S>() -> impl Extractor<
    Output = (GraphQLRequest<S>,), //
    Error = Error,
    Extract = impl TryFuture<Ok = (GraphQLRequest<S>,), Error = Error> + Send + 'static,
>
where
    S: ScalarValue + Send + 'static,
    for<'a> &'a S: ScalarRefValue<'a>,
{
    #[allow(missing_debug_implementations)]
    #[derive(Copy, Clone)]
    enum RequestKind {
        Json,
        GraphQL,
    }

    #[allow(missing_debug_implementations)]
    enum State {
        Init,
        Receive(Concat2<RequestBody>, RequestKind),
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

                        let read_all = RequestBody::take_from(input.locals)
                            .ok_or_else(|| {
                                tsukuyomi::error::internal_server_error(
                                    "the payload has already stolen by another extractor",
                                )
                            })?
                            .concat2();
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

fn parse_query_request<S>(input: &mut Input<'_>) -> tsukuyomi::Result<GraphQLRequest<S>>
where
    S: ScalarValue,
    for<'a> &'a S: ScalarRefValue<'a>,
{
    let query_str = input
        .request
        .uri()
        .query()
        .ok_or_else(|| GraphQLParseError::MissingQuery)?;
    parse_query_str(query_str).map_err(Into::into)
}

fn parse_query_str<S>(s: &str) -> Result<GraphQLRequest<S>, GraphQLParseError>
where
    S: ScalarValue,
    for<'a> &'a S: ScalarRefValue<'a>,
{
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
#[serde(bound = "InputValue<S>: Deserialize<'de>")]
pub struct GraphQLRequest<S: ScalarValue = DefaultScalarValue>(GraphQLRequestKind<S>);

#[derive(Debug, Deserialize)]
#[serde(untagged, bound = "InputValue<S>: Deserialize<'de>")]
enum GraphQLRequestKind<S: ScalarValue> {
    Single(juniper::http::GraphQLRequest<S>),
    Batch(Vec<juniper::http::GraphQLRequest<S>>),
}

impl<S> GraphQLRequest<S>
where
    S: ScalarValue,
    for<'a> &'a S: ScalarRefValue<'a>,
{
    fn single(
        query: String,
        operation_name: Option<String>,
        variables: Option<InputValue<S>>,
    ) -> Self {
        GraphQLRequest(GraphQLRequestKind::Single(
            juniper::http::GraphQLRequest::new(query, operation_name, variables),
        ))
    }

    /// Creates a `Responder` that executes this request using the specified schema and context.
    pub fn execute<T, CtxT>(self, schema: T, context: CtxT) -> GraphQLResponse<T, CtxT, S>
    where
        T: Schema<S> + Send + 'static,
        CtxT: AsRef<T::Context> + Send + 'static,
        S: Send + 'static,
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
pub struct GraphQLResponse<T, CtxT, S: ScalarValue = DefaultScalarValue> {
    request: GraphQLRequest<S>,
    schema: T,
    context: CtxT,
}

impl<T, CtxT, S> Responder for GraphQLResponse<T, CtxT, S>
where
    T: Schema<S> + Send + 'static,
    CtxT: AsRef<T::Context> + Send + 'static,
    S: ScalarValue + Send + 'static,
    for<'a> &'a S: ScalarRefValue<'a>,
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
        let handle = tsukuyomi_server::rt::spawn_fn(move || -> tsukuyomi::Result<_> {
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
    handle: tsukuyomi_server::rt::SpawnHandle<
        tsukuyomi::Result<Response<Vec<u8>>>,
        tsukuyomi_server::rt::BlockingError,
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
