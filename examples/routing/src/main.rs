extern crate tsukuyomi;

use tsukuyomi::app::{route, scope, App};

fn main() {
    let app = App::builder()
        .route(
            route!() //
                .reply(|| "Hello, world\n"),
        ) //
        .mount(
            scope::with_prefix("/api/v1/")
                .mount(
                    scope::with_prefix("/posts")
                        .route(route!().reply(|| "list_posts"))
                        .route(route!("/:id").reply(|id: i32| format!("get_post(id = {})", id)))
                        .route(route!("/", method = POST).reply(|| "add_post")),
                ) //
                .mount(
                    scope::with_prefix("/user") //
                        .route(route!("/auth").reply(|| "Authentication")),
                ),
        ) //
        .route(
            route!("/static/*path")
                .reply(|path: std::path::PathBuf| format!("path = {}\n", path.display())),
        ) //
        .finish()
        .unwrap();

    tsukuyomi::server(app).run_forever().unwrap();
}
