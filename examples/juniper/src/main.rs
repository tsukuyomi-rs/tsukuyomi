#![allow(clippy::double_parens)]

mod context;
mod schema;

use {
    crate::context::{Context, Database},
    std::sync::{Arc, RwLock},
    tsukuyomi::{endpoint::builder as endpoint, server::Server, App},
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

    let app = App::build(|s| {
        // renders the source of GraphiQL.
        s.at("/", (), {
            endpoint::get() //
                .reply(tsukuyomi_juniper::graphiql_source("/graphql"))
        })?;

        // modifies all errors that this route throws into GraphQL errors.
        let capture_errors = capture_errors();

        // a route which handles GraphQL requests over HTTP.
        s.at("/graphql", capture_errors, {
            endpoint::allow_only("GET, POST")?
                .extract(tsukuyomi_juniper::request()) // <-- parses the incoming GraphQL request.
                .extract(fetch_graphql_context) // <-- fetches a GraphQL context.
                .call(move |request: GraphQLRequest, context: Context| {
                    // creates a `Responder` that executes a GraphQL request with the specified schema and context.
                    request.execute(schema.clone(), context)
                })
        })
    })?;

    let mut server = Server::new(app)?;
    server.bind("127.0.0.1:4000")?;
    server.run_forever();

    Ok(())
}
