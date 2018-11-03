use diesel::prelude::*;
use futures::prelude::*;

use tsukuyomi::json::{HttpResponse, Json};
use tsukuyomi::{AsyncResponder, Error, Input};

use conn::{get_conn, run_blocking};
use model::Post;

#[derive(Debug, Serialize)]
pub struct Response(Post);

impl HttpResponse for Response {}

pub fn get_post(input: &mut Input) -> impl AsyncResponder<Output = Option<Json<Response>>> {
    let id = input.params()[0].parse::<i32>().map_err(Error::bad_request);

    let conn = input
        .get()
        .ok_or_else(|| Error::internal_server_error(format_err!("missing DB pool")))
        .map(|pool| get_conn(pool))
        .into_future()
        .and_then(|conn| conn.map_err(Error::internal_server_error));

    (id, conn)
        .into_future()
        .and_then(|(id, conn)| {
            run_blocking(move || {
                use schema::posts::dsl;
                dsl::posts.filter(dsl::id.eq(id)).get_result::<Post>(&*conn).optional()
            }).map_err(Error::internal_server_error)
        })
        .map(|post| post.map(|post| Json(Response(post))))
}
