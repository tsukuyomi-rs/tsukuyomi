extern crate tsukuyomi;

use tsukuyomi::app::{App, Route};
use tsukuyomi::route;

fn main() {
    let app = App::builder()
        .route(
            Route::index() //
                .reply(|| "Hello, world\n"),
        ) //
        .mount("/api/v1/", |scope| {
            scope
                .mount("/posts", |scope| {
                    scope
                        .route(Route::index().reply(|| "list_posts"))
                        .route(route!("/:id").reply(|id: i32| format!("get_post(id = {})", id)))
                        .route(route!("/", methods = ["POST"]).reply(|| "add_post"))
                        .done()
                })? //
                .mount("/user", |scope| {
                    scope
                        .route(Route::get("/auth").reply(|| "Authentication"))
                        .done()
                })? //
                .done()
        }).unwrap() //
        .route(
            route!("/*path")
                .reply(|path: std::path::PathBuf| format!("path = {}\n", path.display())),
        ) //
        .finish()
        .unwrap();

    tsukuyomi::server(app).run_forever().unwrap();
}
