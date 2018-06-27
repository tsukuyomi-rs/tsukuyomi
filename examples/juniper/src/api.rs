use futures::future::poll_fn;
use futures::sync::oneshot;
use futures::{Async, Future};
use http::{header, Response, StatusCode};
use serde_json;
use std::sync::Arc;
use tokio_executor;
use tokio_threadpool::blocking;

use juniper::http::graphiql::graphiql_source;
use juniper::http::GraphQLRequest;

use tsukuyomi::json::Json;
use tsukuyomi::{Error, Input};

use schema::{Context, Schema};

pub struct GraphQLState {
    pub ctx: Context,
    pub schema: Schema,
}

impl GraphQLState {
    pub fn execute(
        this: Arc<Self>,
        request: GraphQLRequest,
    ) -> impl Future<Item = (String, bool), Error = Error> + Send + 'static {
        let (tx, rx) = oneshot::channel();
        let mut tx_opt = Some(tx);
        tokio_executor::spawn(poll_fn(move || {
            let result = match blocking(|| request.execute(&this.schema, &this.ctx)) {
                Ok(Async::NotReady) => return Ok(Async::NotReady),
                Ok(Async::Ready(response)) => {
                    let is_ok = response.is_ok();
                    serde_json::to_string(&response)
                        .map(|body| (body, is_ok))
                        .map_err(Error::internal_server_error)
                }
                Err(err) => Err(Error::internal_server_error(err)),
            };
            let _ = tx_opt.take().unwrap().send(result);
            Ok(().into())
        }));

        rx.map_err(|_| Error::internal_server_error(format_err!("rx")))
            .and_then(|result| result)
    }
}

#[derive(Debug, Deserialize, Serialize)]
pub struct ApiRequest(GraphQLRequest);

pub fn graphql(input: &mut Input) -> impl Future<Item = Response<String>, Error = Error> + Send + 'static {
    let request = input.body_mut().read_all().convert_to();

    let result = request.and_then(|Json(request): Json<GraphQLRequest>| {
        let state: Arc<GraphQLState> = Input::with_current(|input| input.get::<Arc<GraphQLState>>().clone());
        GraphQLState::execute(state, request)
    });

    result.and_then(|(body, is_ok)| {
        let mut response = Response::builder();
        if !is_ok {
            response.status(StatusCode::BAD_REQUEST);
        }
        response.header(header::CONTENT_TYPE, "application/json");
        response.body(body).map_err(Error::internal_server_error)
    })
}

pub fn graphiql(_: &mut Input) -> ::tsukuyomi::Result<Response<String>> {
    // TODO: add Responder for representing HTML responses.
    let body = graphiql_source("http://127.0.0.1:4000/graphql");
    Response::builder()
        .header(header::CONTENT_TYPE, "text/html; charset=utf-8")
        .body(body)
        .map_err(Error::internal_server_error)
}
