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
        config::prelude::*, //
        error::Error,
        extractor::{self, ExtractorExt},
        App,
        Server,
    },
};

fn main() -> tsukuyomi::server::Result<()> {
    pretty_env_logger::init();
    dotenv()?;

    let database_url = env::var("DATABASE_URL")?;
    let db_conn = crate::conn::extractor(database_url).map(Arc::new)?;

    let server = App::create({
        mount("/api/v1/posts").with(chain![
            path!("/") //
                .to(chain![
                    endpoint::get()
                        .extract(db_conn.clone())
                        .extract(extractor::query().optional())
                        .call_async({
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
                        }),
                    endpoint::post() //
                        .extract(db_conn.clone())
                        .extract(extractor::body::json())
                        .call_async({
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
                        }),
                ]),
            path!("/:id") //
                .to(endpoint::get() //
                    .extract(db_conn)
                    .call_async(|id: i32, conn: Conn| blocking_section(move || {
                        use crate::schema::posts::dsl;
                        use diesel::prelude::*;
                        dsl::posts
                            .filter(dsl::id.eq(id))
                            .get_result::<Post>(&*conn)
                            .optional()
                            .map_err(tsukuyomi::error::internal_server_error)
                    })
                    .map(|post_opt| post_opt.map(tsukuyomi::output::json))))
        ])
    })
    .map(Server::new)?;

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
