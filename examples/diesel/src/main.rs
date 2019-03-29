#![allow(proc_macro_derive_resolution_fallback)]

#[macro_use]
extern crate diesel;

mod conn;
mod model;
mod schema;

use {
    crate::{
        conn::Conn,
        model::{NewPost, Post},
    },
    dotenv::dotenv,
    futures::Future,
    std::{env, sync::Arc},
    tsukuyomi::{
        endpoint,
        error::Error,
        extractor::{self, ExtractorExt},
        path,
        server::Server,
        App,
    },
};

fn main() -> failure::Fallible<()> {
    pretty_env_logger::init();
    dotenv()?;

    let database_url = env::var("DATABASE_URL")?;
    let db_conn = crate::conn::extractor(database_url).map(Arc::new)?;

    let list_posts = {
        #[derive(Debug, serde::Deserialize)]
        struct Param {
            #[serde(default)]
            count: i64,
        }
        |conn: Conn, param: Option<Param>| {
            let param = param.unwrap_or_else(|| Param { count: 20 });
            blocking_section(move || {
                use crate::schema::posts::dsl::*;
                use diesel::prelude::*;
                posts
                    .limit(param.count)
                    .load::<Post>(&*conn)
                    .map_err(tsukuyomi::error::internal_server_error)
            })
            .map(tsukuyomi::output::json)
        }
    };

    let create_post = {
        #[derive(Debug, serde::Deserialize)]
        struct Param {
            title: String,
            body: String,
        }
        |conn: Conn, param: Param| {
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
            })
            .map(|_| ())
        }
    };

    let fetch_post = |id: i32, conn: Conn| {
        blocking_section(move || {
            use crate::schema::posts::dsl;
            use diesel::prelude::*;
            dsl::posts
                .filter(dsl::id.eq(id))
                .get_result::<Post>(&*conn)
                .optional()
                .map_err(tsukuyomi::error::internal_server_error)?
                .ok_or_else(|| tsukuyomi::error::not_found("missing post"))
        })
        .map(tsukuyomi::output::json)
    };

    let app = App::build(|mut scope| {
        scope.mount("/api/v1/posts")?.done(|mut scope| {
            scope.at("/")?.done(|mut resource| {
                resource
                    .get()
                    .extract(db_conn.clone())
                    .extract(extractor::query().optional())
                    .to(endpoint::call_async(list_posts))?;

                resource
                    .post()
                    .extract(db_conn.clone())
                    .extract(extractor::body::json())
                    .to(endpoint::call_async(create_post))
            })?;

            scope
                .at(path!("/:id"))?
                .get()
                .extract(db_conn)
                .to(endpoint::call_async(fetch_post))?;

            Ok(())
        })
    })?;

    let mut server = Server::new(app)?;
    server.bind("127.0.0.1:4000")?;
    server.run_forever();

    Ok(())
}

fn blocking_section<F, T, E>(op: F) -> impl Future<Error = Error, Item = T>
where
    F: FnOnce() -> Result<T, E>,
    E: Into<Error>,
{
    izanami::rt::blocking_section(op).then(|result| {
        result
            .map_err(tsukuyomi::error::internal_server_error) // <-- BlockingError
            .and_then(|result| {
                result.map_err(Into::into) // <-- E
            })
    })
}
