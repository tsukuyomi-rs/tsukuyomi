#![warn(unused_extern_crates)]

extern crate tsukuyomi;
extern crate tsukuyomi_juniper;

#[macro_use]
extern crate juniper;

mod context;
mod schema;

use tsukuyomi::App;
use tsukuyomi_juniper::endpoint::{graphiql, GraphQLEndpoint};
use tsukuyomi_juniper::GraphQLState;

fn main() -> tsukuyomi::AppResult<()> {
    let context = context::Context::default();
    let schema = schema::create_schema();
    let state = GraphQLState::new(context, schema);

    let app = App::builder()
        .scope(GraphQLEndpoint::new(state, "/graphql"))
        .route(("/graphiql", graphiql("http://127.0.0.1:4000/graphql")))
        .finish()?;

    tsukuyomi::run(app)
}
