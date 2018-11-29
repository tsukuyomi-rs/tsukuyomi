extern crate tsukuyomi;

use tsukuyomi::app::{mount, route};

fn main() -> tsukuyomi::server::Result<()> {
    tsukuyomi::app!()
        .with(
            route!() //
                .say("Hello, world\n"),
        ) //
        .with(
            mount!("/api/v1/")
                .with(
                    mount!("/posts")
                        .with(route!("/").say("list_posts"))
                        .with(route!("/:id").reply(|id: i32| format!("get_post(id = {})", id)))
                        .with(route!("/", method = POST).say("add_post")),
                ) //
                .with(
                    mount!("/user") //
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
