extern crate tsukuyomi;

use tsukuyomi::app::{route, scope};

fn main() {
    tsukuyomi::app()
        .route(
            route!() //
                .reply(|| "Hello, world\n"),
        ) //
        .mount(
            scope!("/api/v1/")
                .mount(
                    scope!("/posts")
                        .route(route!("/").reply(|| "list_posts"))
                        .route(route!("/:id").reply(|id: i32| format!("get_post(id = {})", id)))
                        .route(route!("/", method = POST).reply(|| "add_post")),
                ) //
                .mount(
                    scope!("/user") //
                        .route(route!("/auth").reply(|| "Authentication")),
                ),
        ) //
        .route(
            route!("/static/*path")
                .reply(|path: std::path::PathBuf| format!("path = {}\n", path.display())),
        ) //
        .build_server()
        .unwrap()
        .run_forever()
        .unwrap();
}
