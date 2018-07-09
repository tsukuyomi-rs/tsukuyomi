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
use tsukuyomi_juniper::{graphiql_endpoint, GraphQLRequest, GraphQLResponse, GraphQLState};

use graphql::{Context, Mutation, Query};

type Cx = GraphQLState<Query, Mutation, Context>;

fn main() -> tsukuyomi::AppResult<()> {
    let cx = GraphQLState::new(graphql::create_schema(), Context::default());

    let app = App::builder()
        .manage(cx)
        .mount("/", |m| {
            m.get("/graphiql")
                .handle(graphiql_endpoint("http://127.0.0.1:4000/graphql"));

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
        let cx = input.get::<Cx>();
        cx.execute(request)
    });

    future
}
