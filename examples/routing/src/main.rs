use {
    std::path::PathBuf,
    tsukuyomi::{
        config::prelude::*, //
        App,
    },
    tsukuyomi_server::Server,
};

fn main() -> tsukuyomi_server::Result<()> {
    App::create(chain![
        // a route that matches the root path.
        path!("/") //
            .to({
                // an endpoint that matches *all* methods with the root path.
                endpoint::any() //
                    .reply("Hello, world\n") // replies by cloning a `Responder`.
            }),
        // a sub-scope with the prefix `/api/v1/`.
        mount("/api/v1/").with(chain![
            // scopes can be nested.
            mount("/posts").with(chain![
                // a route with the path `/api/v1/posts`.
                path!("/") //
                    .to(chain![
                        // A route can take multiple endpoints by using the `chain!()`.
                        //
                        // If there are multiple endpoint matching the same method, the one specified earlier will be chosen.
                        endpoint::get().reply("list_posts"), // <-- GET /api/v1/posts
                        endpoint::post().reply("add_post"),  // <-- POST /api/v1/posts
                        endpoint::any().reply("other methods"), // <-- {PUT, DELETE, ...} /api/v1/posts
                    ]),
                // A route that captures a parameter from the path.
                path!("/:id") //
                    .to({
                        endpoint::any() //
                            .call(|id: i32| {
                                // returns a `Responder`.
                                format!("get_post(id = {})", id)
                            })
                    }),
            ]),
            mount("/user").with({
                path!("/auth") //
                    .to(endpoint::any().reply("Authentication"))
            }),
        ]),
        // a route that captures a *catch-all* parameter.
        path!("/static/*path") //
            .to({
                endpoint::get() //
                    .call(|path: PathBuf| {
                        // returns a `Future` which will return a `Responder`.
                        tsukuyomi::fs::NamedFile::open(path)
                    })
            }),
        // A route that matches any path.
        path!("*") //
            .to(endpoint::any().reply("default route"))
    ])
    .map(App::into_service)
    .map(Server::new)?
    .run()
}
