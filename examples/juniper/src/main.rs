#![warn(unused)]
#![cfg_attr(feature = "cargo-clippy", allow(double_parens))]

extern crate juniper;
extern crate tsukuyomi;
extern crate tsukuyomi_juniper;

mod context;
mod schema;

use std::sync::Arc;
use tsukuyomi_juniper::executor::Executor;

fn main() {
    let context = Arc::new(crate::context::Context::default());

    let extract_graphql_executor = {
        let schema = crate::schema::create_schema();
        let extractor = tsukuyomi_juniper::executor(schema);
        Arc::new(extractor)
    };

    let app = tsukuyomi::app(|scope| {
        scope.route(tsukuyomi::route!().reply(tsukuyomi_juniper::graphiql("/graphql")));

        scope.route(
            tsukuyomi::route!("/graphql", methods = ["GET", "POST"])
                .with(extract_graphql_executor.clone())
                .handle({
                    let context = context.clone();
                    move |exec: Executor<_>| exec.execute(context.clone())
                }),
        );
    }).unwrap();

    tsukuyomi::server(app)
        .bind("127.0.0.1:4000")
        .run_forever()
        .unwrap();
}
