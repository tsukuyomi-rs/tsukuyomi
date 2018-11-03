#![warn(unused_extern_crates)]

#[macro_use]
extern crate diesel;
extern crate dotenv;
#[macro_use]
extern crate failure;
extern crate futures;
extern crate http;
#[macro_use]
extern crate serde;
extern crate pretty_env_logger;
extern crate serde_qs;
extern crate tsukuyomi;

mod api;
mod conn;
mod model;
mod schema;

use dotenv::dotenv;
use http::Method;
use std::env;
use tsukuyomi::handler::wrap_async;
use tsukuyomi::App;

fn main() -> tsukuyomi::AppResult<()> {
    pretty_env_logger::init();
    dotenv()?;

    let pool = conn::init_pool(env::var("DATABASE_URL")?)?;

    let app = App::builder()
        .mount("/api/v1/posts", |m| {
            m.set(pool);
            m.route(("/", Method::GET, wrap_async(api::get_posts)));
            m.route(("/", Method::POST, wrap_async(api::create_post)));
            m.route(("/:id", Method::GET, wrap_async(api::get_post)));
        })
        .finish()?;

    tsukuyomi::run(app)
}
