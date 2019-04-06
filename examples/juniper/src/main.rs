#![allow(clippy::double_parens)]

mod context;
mod schema;

use {
    crate::context::{Context, Database},
    http::Method,
    std::sync::{Arc, RwLock},
    tsukuyomi::{endpoint, server::Server, App},
    tsukuyomi_juniper::{capture_errors, GraphQLRequest},
};

fn main() -> Result<(), exitfailure::ExitFailure> {
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

    let app = App::builder()
        .root(|mut s| {
            // renders the source of GraphiQL.
            let source = tsukuyomi_juniper::graphiql_source("/graphql");
            s.at("/")?
                .get()
                .to(endpoint::call(move || source.clone()))?;

            // a route which handles GraphQL requests over HTTP.
            s.at("/graphql")?
                .route(&[Method::GET, Method::POST])
                .with(capture_errors()) // modifies all errors that this route throws into GraphQL errors.
                .extract(tsukuyomi_juniper::request()) // <-- parses the incoming GraphQL request.
                .extract(fetch_graphql_context) // <-- fetches a GraphQL context.
                .to(endpoint::call(
                    move |request: GraphQLRequest, context: Context| {
                        // creates a `Responder` that executes a GraphQL request with the specified schema and context.
                        request.execute(schema.clone(), context)
                    },
                ))
        })?
        .build()?;

    let mut server = Server::new(app)?;
    server.bind("127.0.0.1:4000")?;
    server.run_forever();

    Ok(())
}
