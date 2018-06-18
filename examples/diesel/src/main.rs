#[macro_use]
extern crate diesel;
extern crate dotenv;
extern crate failure;
extern crate futures;
#[macro_use]
extern crate serde;
extern crate tokio_executor;
extern crate tokio_threadpool;
extern crate tsukuyomi;

mod api;
mod conn;
mod model;
mod schema;

use dotenv::dotenv;
use std::env;
use tsukuyomi::App;

fn main() -> tsukuyomi::AppResult<()> {
    dotenv()?;

    let pool = conn::init_pool(env::var("DATABASE_URL")?)?;

    let app = App::builder()
        .manage(pool)
        .mount("/posts", |r| {
            r.get("/").handle_async(api::get_posts);
        })
        .finish()?;

    tsukuyomi::run(app)
}
