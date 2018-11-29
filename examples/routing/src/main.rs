extern crate tsukuyomi;

use tsukuyomi::{
    app::{route, scope::mount},
    uri,
};

fn main() -> tsukuyomi::server::Result<()> {
    tsukuyomi::app!()
        .with(
            route!() //
                .say("Hello, world\n"),
        ) //
        .with(
            mount(uri!("/api/v1/"))
                .with(
                    mount(uri!("/posts"))
                        .with(route!("/").say("list_posts"))
                        .with(route!("/:id").reply(|id: i32| format!("get_post(id = {})", id)))
                        .with(route!("/", method = POST).say("add_post")),
                ) //
                .with(
                    mount(uri!("/user")) //
                        .with(route!("/auth").say("Authentication")),
                ),
        ) //
        .with(
            route!("/static/*path")
                .reply(|path: std::path::PathBuf| format!("path = {}\n", path.display())),
        ) //
        .build_server()?
        .run()
}
