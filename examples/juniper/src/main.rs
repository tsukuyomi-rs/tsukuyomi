#[macro_use]
extern crate failure;
extern crate http;
extern crate tsukuyomi;
#[macro_use]
extern crate juniper;
#[macro_use]
extern crate serde;
extern crate futures;
extern crate serde_json;
extern crate tokio_executor;
extern crate tokio_threadpool;

mod api;
mod schema;

use std::sync::Arc;
use tsukuyomi::{App, Handler};

fn main() -> tsukuyomi::AppResult<()> {
    let state = Arc::new(api::GraphQLState {
        ctx: schema::Context::default(),
        schema: schema::create_schema(),
    });

    let app = App::builder()
        .manage(state)
        .mount("/", |m| {
            m.get("/graphiql").handle(Handler::new_ready(api::graphiql));
            m.post("/graphql").handle(Handler::new_async(api::graphql));
        })
        .finish()?;

    tsukuyomi::run(app)
}
