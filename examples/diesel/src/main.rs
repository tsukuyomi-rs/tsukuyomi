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
        chain,
        endpoint::builder as endpoint,
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

    let app = App::build(|s| {
        s.nest("/api/v1/posts", (), |s| {
            s.at("/", (), {
                chain![
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
                ]
            })?;

            s.at(path!("/:id"), (), {
                endpoint::get() //
                    .extract(db_conn)
                    .call_async(|id: i32, conn: Conn| {
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
                    })
            })
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
