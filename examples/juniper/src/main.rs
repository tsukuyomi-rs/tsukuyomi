#![warn(unused)]
#![cfg_attr(feature = "cargo-clippy", allow(double_parens))]

extern crate juniper;
extern crate tsukuyomi;
extern crate tsukuyomi_juniper;

mod context;
mod schema;

use std::sync::Arc;
use tsukuyomi_juniper::Executor;

fn main() -> tsukuyomi::server::Result<()> {
    // Extractor for extracting `Executor` for executing a GraphQL request from client.
    let extract_graphql_executor = tsukuyomi_juniper::executor(
        //
        crate::schema::create_schema(),
    );

    // Extractor which constructs a context value used by `Executor`.
    let fetch_graphql_context = {
        let context = Arc::new(crate::context::Context::default());
        tsukuyomi::extractor::value(context)
    };

    tsukuyomi::app()
        .route(
            tsukuyomi::app::route!("/") //
                .reply({
                    let source = tsukuyomi_juniper::graphiql_source("/graphql");
                    move || source.clone()
                }),
        ) //
        .route(
            tsukuyomi::app::route!("/graphql", methods = [GET, POST])
                .with(extract_graphql_executor)
                .with(fetch_graphql_context)
                .handle(move |exec: Executor<_>, context| exec.execute(context)),
        ) //
        .build_server()?
        .run_forever()
}
