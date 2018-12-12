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
    std::{env, sync::Arc},
    tsukuyomi::{
        app::config::prelude::*, //
        chain,
        error::Error,
        extractor,
        rt::Future,
        server::Server,
        App,
    },
};

fn main() -> tsukuyomi::server::Result<()> {
    pretty_env_logger::init();
    dotenv()?;

    let database_url = env::var("DATABASE_URL")?;
    let db_conn = crate::conn::extractor(database_url).map(Arc::new)?;

    let server = App::create_with_prefix(
        "/api/v1/posts",
        chain![
            route() //
                .extract(db_conn.clone())
                .to(chain![
                    endpoint::get()
                        .extract(extractor::ExtractorExt::new(extractor::query::query()).optional())
                        .call({
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
                    endpoint::post().extract(extractor::body::json()).call({
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
            (route().param("id")?)
                .extract(db_conn)
                .to(
                    endpoint::get().call(|id: i32, conn: Conn| blocking_section(move || {
                        use crate::schema::posts::dsl;
                        use diesel::prelude::*;
                        dsl::posts
                            .filter(dsl::id.eq(id))
                            .get_result::<Post>(&*conn)
                            .optional()
                            .map_err(tsukuyomi::error::internal_server_error)
                    })
                    .map(|post_opt| post_opt.map(tsukuyomi::output::json))),
                )
        ],
    )
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
