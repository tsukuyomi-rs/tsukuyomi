extern crate tsukuyomi;

use tsukuyomi::app::App;
use tsukuyomi::route;

fn main() {
    let app = App::builder()
        .route(route::index().reply(|| "Hello, world\n"))
        .mount("/api/v1/", |scope| {
            scope.mount("/posts", |scope| {
                scope.route(route::index().reply(|| "list_posts"));
                scope.route(route::get!("/:id").reply(|id: i32| format!("get_post(id = {})", id)));
                scope.route(route::post("/").reply(|| "add_post"));
            });
            scope.mount("/user", |scope| {
                scope.route(route::get("/auth").reply(|| "Authentication"));
            });
        }).route(
            route::get!("/*path")
                .reply(|path: std::path::PathBuf| format!("path = {}\n", path.display())),
        ).finish()
        .unwrap();

    tsukuyomi::server::server(app)
        .bind("127.0.0.1:4000")
        .run_forever()
        .unwrap();
}
