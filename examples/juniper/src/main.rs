#![allow(clippy::double_parens)]

mod context;
mod schema;

use {
    crate::context::{Context, Database},
    futures::prelude::*,
    std::sync::{Arc, RwLock},
    tsukuyomi::{app::config::prelude::*, chain, server::Server, App},
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

    App::create(chain![
        // renders the source of GraphiQL.
        path!(/) //
            .to(endpoint::get() //
                .reply(tsukuyomi_juniper::graphiql_source("/graphql"))),
        // a route which handles GraphQL requests over HTTP.
        path!(/"graphql")
            .to(endpoint::allow_only("GET, POST")?
                .extract(tsukuyomi_juniper::request()) // <-- parses the incoming GraphQL request.
                .extract(fetch_graphql_context) // <-- fetches a GraphQL context.
                .call_async(move |request: GraphQLRequest, context: Context| {
                    // spawns a task that executes the (parsed) GraphQL request.
                    tsukuyomi::rt::spawn_fn({
                        let schema = schema.clone();
                        move || request.execute(&schema, &context)
                    })
                    .map_err(tsukuyomi::error::internal_server_error)
                }))
            .modify(GraphQLModifier::default()) // <-- modifies all errors thrown from this route into GraphQL error.
    ])
    .map(Server::new)?
    .run()
}
