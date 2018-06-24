#[macro_use]
extern crate diesel;
extern crate dotenv;
extern crate failure;
extern crate futures;
extern crate http;
#[macro_use]
extern crate serde;
extern crate pretty_env_logger;
extern crate serde_qs;
extern crate tokio_executor;
extern crate tokio_threadpool;
extern crate tsukuyomi;

mod api;
mod conn;
mod model;
mod schema;

use dotenv::dotenv;
use std::env;
use tsukuyomi::{App, Handler};

fn main() -> tsukuyomi::AppResult<()> {
    pretty_env_logger::init();
    dotenv()?;

    let pool = conn::init_pool(env::var("DATABASE_URL")?)?;

    let app = App::builder()
        .manage(pool)
        .mount("/api/v1/posts", |m| {
            m.get("/").handle(Handler::new_async(api::get_posts));
            m.post("/").handle(Handler::new_async(api::create_post));
            m.get("/:id").handle(Handler::new_async(api::get_post));
        })
        .finish()?;

    tsukuyomi::run(app)
}
