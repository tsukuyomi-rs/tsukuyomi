use tsukuyomi::{
    app::config::prelude::*, //
    chain,
    server::Server,
    App,
};

fn main() -> tsukuyomi::server::Result<()> {
    App::create(chain![
        path!(/) //
            .to(endpoint::any().say("Hello, world\n")),
        mount("/api/v1/").with(chain![
            mount("/posts").with(chain![
                path!(/) //
                    .to(chain![
                        endpoint::get().say("list_posts"),
                        endpoint::post().say("add_post"),
                    ]),
                path!(/ {path::param("id")}) //
                    .to(endpoint::any().reply(|id: i32| format!("get_post(id = {})", id))),
            ]),
            mount("/user").with({
                path!(/ "auth") //
                    .to(endpoint::any().say("Authentication"))
            }),
        ]),
        path!(/"static"/{path::catch_all("path")}) //
            .to(endpoint::get()
                .reply(|path: std::path::PathBuf| format!("path = {}\n", path.display()))),
    ])
    .map(Server::new)?
    .run()
}
