use {
    crate::{error::GraphQLParseError, Schema},
    futures::{Async, Future},
    http::{Method, Response, StatusCode},
    juniper::InputValue,
    percent_encoding::percent_decode,
    tsukuyomi::{
        error::Error,
        extractor::{ExtractStatus, Extractor},
        input::Input,
        output::Responder,
    },
};

/// Create an `Extractor` that parses the incoming request as GraphQL query.
pub fn request() -> impl Extractor<Output = (GraphQLRequest,), Error = Error> {
    tsukuyomi::extractor::raw(|input| -> tsukuyomi::Result<_> {
        if input.request.method() == Method::GET {
            let query_str = input
                .request
                .uri()
                .query()
                .ok_or_else(|| GraphQLParseError::MissingQuery)?;
            let request = parse_query_str(query_str)?;
            return Ok(ExtractStatus::Ready((request,)));
        }

        if input.request.method() == Method::POST {
            #[allow(missing_debug_implementations)]
            enum RequestKind {
                Json,
                GraphQL,
            }

            let kind = match input.content_type()? {
                Some(mime) if *mime == mime::APPLICATION_JSON => RequestKind::Json,
                Some(mime) if *mime == "application/graphql" => RequestKind::GraphQL,
                Some(..) => return Err(GraphQLParseError::InvalidMime.into()),
                None => return Err(GraphQLParseError::MissingMime.into()),
            };

            let mut read_all = input
                .body()
                .ok_or_else(|| {
                    tsukuyomi::error::internal_server_error(
                        "the payload has already stolen by another extractor",
                    )
                })?.read_all();
            let future = futures::future::poll_fn(move || match kind {
                RequestKind::Json => {
                    let data = futures::try_ready!(read_all.poll().map_err(Error::critical));
                    let request =
                        serde_json::from_slice(&*data).map_err(GraphQLParseError::ParseJson)?;
                    Ok(Async::Ready((request,)))
                }
                RequestKind::GraphQL => {
                    let data = futures::try_ready!(read_all.poll().map_err(Error::critical));
                    String::from_utf8(data.to_vec())
                        .map(|query| Async::Ready((GraphQLRequest::single(query, None, None),)))
                        .map_err(|e| GraphQLParseError::DecodeUtf8(e.utf8_error()).into())
                }
            });

            return Ok(ExtractStatus::Pending(future));
        }

        Err(GraphQLParseError::InvalidRequestMethod.into())
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

    /// Executes this request using the specified schema and context.
    ///
    /// Note that this method will block the current thread due to the restriction of Juniper.
    pub fn execute<S>(self, schema: &S, context: &S::Context) -> GraphQLResponse
    where
        S: Schema,
    {
        use self::GraphQLRequestKind::*;
        match self.0 {
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

/// The type representing the result from the executing a GraphQL request.
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
