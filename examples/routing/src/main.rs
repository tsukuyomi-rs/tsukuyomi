extern crate tsukuyomi;

use tsukuyomi::app::{scope, App};
use tsukuyomi::route;

fn main() {
    let app = App::builder()
        .route(
            route!() //
                .reply(|| "Hello, world\n"),
        ) //
        .mount("/api/v1/", {
            scope::builder()
                .mount(
                    "/posts",
                    scope::builder()
                        .route(route!().reply(|| "list_posts"))
                        .route(route!("/:id").reply(|id: i32| format!("get_post(id = {})", id)))
                        .route(route!("/", method = POST).reply(|| "add_post")),
                ) //
                .mount(
                    "/user",
                    scope::builder().route(route!("/auth").reply(|| "Authentication")),
                )
        }) //
        .route(
            route!("/*path")
                .reply(|path: std::path::PathBuf| format!("path = {}\n", path.display())),
        ) //
        .finish()
        .unwrap();

    tsukuyomi::server(app).run_forever().unwrap();
}
