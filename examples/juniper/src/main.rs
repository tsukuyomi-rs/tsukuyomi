#![cfg_attr(feature = "cargo-clippy", allow(double_parens))]

extern crate futures;
extern crate juniper;
extern crate tsukuyomi;
extern crate tsukuyomi_juniper;

mod context;
mod schema;

use {
    crate::context::{Context, Database},
    futures::prelude::*,
    std::sync::{Arc, RwLock},
    tsukuyomi::app::directives::*,
    tsukuyomi_juniper::{GraphQLModifier, GraphQLRequest},
};

fn main() -> tsukuyomi::server::Result<()> {
    // A GraphQL schema.
    let schema = Arc::new(crate::schema::create_schema());

    // Extractor which creates a GraphQL context from the request.
    let fetch_graphql_context = {
        let database = Arc::new(RwLock::new(Database::default()));
        tsukuyomi::extractor::ready(move |_| -> tsukuyomi::Result<Context> {
            Ok(Context {
                database: database.clone(),
            })
        })
    };

    App::builder()
        .with(
            // renders the source of GraphiQL.
            route!("/") //
                .say(tsukuyomi_juniper::graphiql_source("/graphql")),
        ) //
        .with(
            // the endpoint which handles GraphQL requests over HTTP.
            route!("/graphql")
                .methods("GET, POST")?
                .extract(tsukuyomi_juniper::request()) // <-- parses the incoming GraphQL request.
                .extract(fetch_graphql_context) // <-- fetches a GraphQL context.
                .modify(GraphQLModifier::default()) // <-- modifies all errors in the route to a GraphQL error.
                .call(move |request: GraphQLRequest, context: Context| {
                    // spawns a task that executes the (parsed) GraphQL request.
                    tsukuyomi::rt::spawn_fn({
                        let schema = schema.clone();
                        move || request.execute(&schema, &context)
                    }).map_err(tsukuyomi::error::internal_server_error)
                }),
        ) //
        .build_server()?
        .run()
}
