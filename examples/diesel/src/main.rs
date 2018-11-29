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

use {
    crate::{
        conn::Conn,
        model::{NewPost, Post},
    },
    dotenv::dotenv,
    std::{env, sync::Arc},
    tsukuyomi::{
        app::scope::route,
        error::Error,
        extractor::{self, Extractor},
        rt::Future,
    },
};

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
            .extract(parse_query)
            .extract(db_conn.clone())
            .call(|param: Param, conn: Conn| {
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
            .extract(extractor::body::json())
            .extract(db_conn.clone())
            .call(|param: Param, conn: Conn| {
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
        .extract(db_conn)
        .call(|id: i32, conn: Conn| {
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

    let server = tsukuyomi::App::with_prefix(tsukuyomi::uri!("/api/v1/posts"))
        .with(get_posts)
        .with(create_post)
        .with(get_post)
        .build_server()?;

    server.run()
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
