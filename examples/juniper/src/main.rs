#![warn(unused_extern_crates)]

extern crate tsukuyomi;
extern crate tsukuyomi_juniper;

#[macro_use]
extern crate juniper;

mod context;
mod schema;

use tsukuyomi::App;
use tsukuyomi_juniper::{AppGraphQLExt as _AppGraphQLExt, GraphQLState};

fn main() -> tsukuyomi::AppResult<()> {
    let context = context::Context::default();
    let schema = schema::create_schema();
    let state = GraphQLState::new(context, schema);

    let app = App::builder()
        .graphql("/graphql", state)
        .graphiql("/graphiql", "http://127.0.0.1:4000/graphql")
        .finish()?;

    tsukuyomi::run(app)
}
