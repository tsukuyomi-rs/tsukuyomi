extern crate tsukuyomi;

use tsukuyomi::app::{route, scope};

fn main() -> tsukuyomi::server::Result<()> {
    tsukuyomi::app!()
        .route(
            route!() //
                .say("Hello, world\n"),
        ) //
        .mount(
            scope!("/api/v1/")
                .mount(
                    scope!("/posts")
                        .route(route!("/").say("list_posts"))
                        .route(route!("/:id").reply(|id: i32| format!("get_post(id = {})", id)))
                        .route(route!("/", method = POST).say("add_post")),
                ) //
                .mount(
                    scope!("/user") //
                        .route(route!("/auth").say("Authentication")),
                ),
        ) //
        .route(
            route!("/static/*path")
                .reply(|path: std::path::PathBuf| format!("path = {}\n", path.display())),
        ) //
        .build_server()?
        .run()
}
