use tsukuyomi::{
    app::config::prelude::*, //
    chain,
    server::Server,
    App,
};

fn main() -> tsukuyomi::server::Result<()> {
    App::configure(chain![
        route().to(endpoint::any().say("Hello, world\n")),
        mount("/api/v1/", ())
            .with(
                mount("/posts", ())
                    .with(route().to(endpoint::any().say("list_posts")),)
                    .with(
                        (route().param("id")?)
                            .to(endpoint::any().reply(|id: i32| format!("get_post(id = {})", id))),
                    )
                    .with(route().to(endpoint::post().say("add_post")))
            )
            .with(
                mount("/user", ())
                    .with((route().segment("auth")?).to(endpoint::any().say("Authentication")),),
            ),
        (route().segment("static")?.catch_all("path")?).to(endpoint::get()
            .reply(|path: std::path::PathBuf| format!("path = {}\n", path.display()))),
    ])
    .map(Server::new)?
    .run()
}
