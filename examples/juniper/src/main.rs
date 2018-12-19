#![allow(clippy::double_parens)]

mod context;
mod schema;

use {
    crate::context::{Context, Database},
    std::sync::{Arc, RwLock},
    tsukuyomi::{config::prelude::*, App},
    tsukuyomi_juniper::{GraphQLModifier, GraphQLRequest},
    tsukuyomi_server::Server,
};

fn main() -> tsukuyomi_server::Result<()> {
    // A GraphQL schema.
    let schema = Arc::new(crate::schema::create_schema());

    // Extractor which creates a GraphQL context from the request.
    let fetch_graphql_context = {
        let database = Arc::new(RwLock::new(Database::default()));
        tsukuyomi::extractor::ready(move |_| -> tsukuyomi::Result<_> {
            Ok((Context {
                database: database.clone(),
            },))
        })
    };

    let app = App::create(chain![
        // renders the source of GraphiQL.
        path!("/") //
            .to(endpoint::get() //
                .reply(tsukuyomi_juniper::graphiql_source("/graphql"))),
        // a route which handles GraphQL requests over HTTP.
        path!("/graphql")
            .to(endpoint::allow_only("GET, POST")?
                .extract(tsukuyomi_juniper::request()) // <-- parses the incoming GraphQL request.
                .extract(fetch_graphql_context) // <-- fetches a GraphQL context.
                .call(move |request: GraphQLRequest, context: Context| {
                    // creates a `Responder` that executes a GraphQL request with the specified schema and context.
                    request.execute(schema.clone(), context)
                }))
            .modify(GraphQLModifier::default()) // <-- modifies all errors thrown from this route into GraphQL error.
    ])?;

    Server::new(app).run()
}
