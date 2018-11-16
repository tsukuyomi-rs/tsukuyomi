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

use tsukuyomi::app::route;
use tsukuyomi::error::Error;
use tsukuyomi::extractor;
use tsukuyomi::extractor::Extractor;
use tsukuyomi::rt::Future;

use crate::conn::Conn;
use crate::model::{NewPost, Post};

fn main() -> tsukuyomi::server::Result<()> {
    pretty_env_logger::init();
    dotenv()?;

    let database_url = env::var("DATABASE_URL")?;
    let db_conn = crate::conn::extractor(database_url).map(Arc::new)?;

    let get_posts = {
        #[derive(Debug, serde::Deserialize)]
        struct Param {
            #[serde(default)]
            count: i64,
        }

        let parse_query = extractor::query::query()
            .into_builder() // <-- start building
            .optional()
            .map(|param: Option<Param>| param.unwrap_or_else(|| Param { count: 20 }));

        route!("/", method = GET)
            .with(parse_query)
            .with(db_conn.clone())
            .handle(|param: Param, conn: Conn| {
                blocking_section(move || {
                    use crate::schema::posts::dsl::*;
                    use diesel::prelude::*;
                    posts
                        .limit(param.count)
                        .load::<Post>(&*conn)
                        .map_err(tsukuyomi::error::internal_server_error)
                }).map(tsukuyomi::output::json)
            })
    };

    let create_post = {
        #[derive(Debug, serde::Deserialize)]
        struct Param {
            title: String,
            body: String,
        }

        route!("/", method = POST)
            .with(extractor::body::json())
            .with(db_conn.clone())
            .handle(|param: Param, conn: Conn| {
                use crate::schema::posts;
                use diesel::prelude::*;
                blocking_section(move || {
                    let new_post = NewPost {
                        title: &param.title,
                        body: &param.body,
                    };
                    diesel::insert_into(posts::table)
                        .values(&new_post)
                        .execute(&*conn)
                        .map_err(tsukuyomi::error::internal_server_error)
                }).map(|_| ())
            })
    };

    let get_post = route!("/:id", method = GET) //
        .with(db_conn)
        .handle(|id: i32, conn: Conn| {
            blocking_section(move || {
                use crate::schema::posts::dsl;
                use diesel::prelude::*;
                dsl::posts
                    .filter(dsl::id.eq(id))
                    .get_result::<Post>(&*conn)
                    .optional()
                    .map_err(tsukuyomi::error::internal_server_error)
            }).map(|post_opt| post_opt.map(tsukuyomi::output::json))
        });

    let server = tsukuyomi::app!("/api/v1/posts")
        .route(get_posts)
        .route(create_post)
        .route(get_post)
        .build_server()?;

    server.run_forever()
}

fn blocking_section<F, T, E>(op: F) -> impl Future<Error = Error, Item = T>
where
    F: FnOnce() -> Result<T, E>,
    E: Into<Error>,
{
    tsukuyomi::rt::blocking(op).then(|result| {
        result
            .map_err(tsukuyomi::error::internal_server_error) // <-- BlockingError
            .and_then(|result| {
                result.map_err(Into::into) // <-- E
            })
    })
}
