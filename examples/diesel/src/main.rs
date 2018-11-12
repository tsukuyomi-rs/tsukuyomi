#![warn(unused)]
#![allow(proc_macro_derive_resolution_fallback)]

#[macro_use]
extern crate diesel;
extern crate dotenv;
extern crate failure;
extern crate pretty_env_logger;
extern crate serde;
extern crate tsukuyomi;

mod conn;
mod model;
mod schema;

use dotenv::dotenv;
use std::env;
use std::sync::Arc;

use tsukuyomi::app::App;
use tsukuyomi::extractor;
use tsukuyomi::extractor::ExtractorExt;
use tsukuyomi::route;

use crate::conn::Conn;
use crate::model::{NewPost, Post};

fn main() {
    pretty_env_logger::init();
    dotenv().unwrap();

    let database_url = env::var("DATABASE_URL").unwrap();
    let db_conn = crate::conn::extractor(database_url).map(Arc::new).unwrap();

    let get_posts = {
        #[derive(Debug, serde::Deserialize)]
        struct Param {
            #[serde(default)]
            count: i64,
        }

        let parse_query = extractor::query::query()
            .optional()
            .map(|param: Option<Param>| param.unwrap_or_else(|| Param { count: 20 }));

        route!("/", methods = ["GET"])
            .with(parse_query)
            .with(db_conn.clone())
            .handle(|param: Param, conn: Conn| {
                tsukuyomi::rt::blocking_section(move || {
                    use crate::schema::posts::dsl::*;
                    use diesel::prelude::*;
                    posts
                        .limit(param.count)
                        .load::<Post>(&*conn)
                        .map(tsukuyomi::output::json)
                        .map_err(tsukuyomi::error::internal_server_error)
                })
            })
    };

    let create_post = {
        #[derive(Debug, serde::Deserialize)]
        struct Param {
            title: String,
            body: String,
        }

        route!("/", methods = ["POST"])
            .with(extractor::body::json())
            .with(db_conn.clone())
            .handle(|param: Param, conn: Conn| {
                use crate::schema::posts;
                use diesel::prelude::*;
                tsukuyomi::rt::blocking_section(move || {
                    let new_post = NewPost {
                        title: &param.title,
                        body: &param.body,
                    };
                    diesel::insert_into(posts::table)
                        .values(&new_post)
                        .execute(&*conn)
                        .map(|_| ())
                        .map_err(tsukuyomi::error::internal_server_error)
                })
            })
    };

    let get_post = route!("/:id", methods = ["GET"]) //
        .with(db_conn)
        .handle(|id: i32, conn: Conn| {
            tsukuyomi::rt::blocking_section(move || {
                use crate::schema::posts::dsl;
                use diesel::prelude::*;
                dsl::posts
                    .filter(dsl::id.eq(id))
                    .get_result::<Post>(&*conn)
                    .optional()
                    .map(tsukuyomi::output::json)
                    .map_err(tsukuyomi::error::internal_server_error)
            })
        });

    let app = App::builder()
        .mount("/api/v1/posts", vec![get_posts, create_post, get_post])
        .unwrap() //
        .finish()
        .unwrap();

    tsukuyomi::server(app) //
        .run_forever()
        .unwrap();
}
