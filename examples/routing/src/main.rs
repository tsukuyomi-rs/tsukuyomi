use {
    exitfailure::ExitFailure,
    std::path::PathBuf,
    tsukuyomi::{chain, endpoint::builder as endpoint, path, server::Server, App},
};

fn main() -> Result<(), ExitFailure> {
    let app = App::build(|scope| {
        // a route that matches the root path.
        scope.at("/", (), {
            // an endpoint that matches *all* methods with the root path.
            endpoint::reply("Hello, world\n") // replies by cloning a `Responder`.
        })?;

        // a sub-scope with the prefix `/api/v1/`.
        scope.nest("/api/v1/", (), |scope| {
            // scopes can be nested.
            scope.nest("/posts", (), |scope| {
                // a route with the path `/api/v1/posts`.
                scope.at("/", (), {
                    chain![
                        // A route can take multiple endpoints by using the `chain!()`.
                        //
                        // If there are multiple endpoint matching the same method, the one specified earlier will be chosen.
                        endpoint::get().reply("list_posts"), // <-- GET /api/v1/posts
                        endpoint::post().reply("add_post"),  // <-- POST /api/v1/posts
                        endpoint::reply("other methods"),    // <-- {PUT, DELETE, ...} /api/v1/posts
                    ]
                })?;

                // A route that captures a parameter from the path.
                scope.at(path!("/:id"), (), {
                    endpoint::call(|id: i32| {
                        // returns a `Responder`.
                        format!("get_post(id = {})", id)
                    })
                })
            })?;

            scope.nest("/user", (), |scope| {
                scope.at("/auth", (), {
                    endpoint::reply("Authentication") //
                })
            })
        })?;

        // a route that captures a *catch-all* parameter.
        scope.at(path!("/static/*path"), (), {
            endpoint::get() //
                .call(|path: PathBuf| {
                    // returns a `Future` which will return a `Responder`.
                    tsukuyomi::fs::NamedFile::open(path)
                })
        })?;

        // A route that matches any path.
        scope.default((), {
            endpoint::reply("default route") //
        })
    })?;

    let mut server = Server::new(app)?;
    server.bind("127.0.0.1:4000")?;
    server.run_forever();

    Ok(())
}
