#![warn(unused_extern_crates)]

extern crate tsukuyomi;
#[macro_use]
extern crate juniper;
extern crate futures;
extern crate tsukuyomi_juniper;
#[macro_use]
extern crate failure;

mod graphql;

use futures::{Future, IntoFuture};

use tsukuyomi::{App, Error, Handler, Input};
use tsukuyomi_juniper::{graphiql_source, GraphQLContext, GraphQLRequest, GraphQLResponse};

use graphql::{Context, Mutation, Query};

fn main() -> tsukuyomi::AppResult<()> {
    let cx = GraphQLContext::new(graphql::create_schema(), Context::default());

    let app = App::builder()
        .manage(cx)
        .mount("/", |m| {
            m.get("/graphiql")
                .handle(Handler::new_ready(|_| graphiql_source("http://127.0.0.1:4000/graphql")));

            m.get("/graphql").handle(Handler::new_async(|input| {
                let request = input
                    .uri()
                    .query()
                    .ok_or_else(|| Error::bad_request(format_err!("missing query")))
                    .and_then(GraphQLRequest::from_query);
                request.into_future().and_then(do_execute)
            }));

            m.post("/graphql").handle(Handler::new_async(|input| {
                let request = input.body_mut().read_all().convert_to::<GraphQLRequest>();
                request.and_then(do_execute)
            }));
        })
        .finish()?;

    tsukuyomi::run(app)
}

fn do_execute(request: GraphQLRequest) -> impl Future<Item = GraphQLResponse, Error = Error> + Send + 'static {
    let future = Input::with_current(|input| {
        let cx = input.get::<GraphQLContext<Query, Mutation, Context>>();
        cx.execute(request)
    });

    future
}
