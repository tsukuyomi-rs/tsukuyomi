extern crate tsukuyomi;

use tsukuyomi::app::{App, Route, Scope};
use tsukuyomi::route;

fn app() -> tsukuyomi::app::AppResult<App> {
    App::builder()
        .route(
            Route::index() //
                .reply(|| "Hello, world\n"),
        ) //
        .mount("/api/v1/", |scope: &mut Scope| {
            scope
                .mount(
                    "/posts",
                    vec![
                        Route::index().reply(|| "list_posts"),
                        route!("/:id").reply(|id: i32| format!("get_post(id = {})", id)),
                        route!("/", methods = ["POST"]).reply(|| "add_post"),
                    ],
                )? //
                .mount(
                    "/user",
                    vec![Route::get("/auth")?.reply(|| "Authentication")],
                )? //
                .done()
        })? //
        .route(
            route!("/*path")
                .reply(|path: std::path::PathBuf| format!("path = {}\n", path.display())),
        ) //
        .finish()
}

fn main() {
    let app = app().unwrap();
    tsukuyomi::server(app).run_forever().unwrap();
}
