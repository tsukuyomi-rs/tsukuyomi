extern crate tsukuyomi;

use tsukuyomi::route;

fn main() {
    let app = tsukuyomi::app(|scope| {
        scope.route(route::index().reply(|| "Hello, world\n"));
        scope.mount("/api/v1/", |scope| {
            scope.mount("/posts", |scope| {
                scope.route(route!().reply(|| "list_posts"));
                scope.route(route!("/:id").reply(|id: i32| format!("get_post(id = {})", id)));
                scope.route(route!("/", methods = ["POST"]).reply(|| "add_post"));
            });
            scope.mount("/user", |scope| {
                scope.route(route::get("/auth").reply(|| "Authentication"));
            });
        });
        scope.route(
            route!("/*path")
                .reply(|path: std::path::PathBuf| format!("path = {}\n", path.display())),
        );
    }).unwrap();

    tsukuyomi::server(app)
        .bind("127.0.0.1:4000")
        .run_forever()
        .unwrap();
}
