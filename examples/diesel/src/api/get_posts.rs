use diesel::prelude::*;
use futures::prelude::*;

use tsukuyomi::json::Json;
use tsukuyomi::output::HttpResponse;
use tsukuyomi::{App, Error};

use conn::{get_conn, run_blocking};
use model::Post;

#[derive(Debug, Serialize)]
pub struct Response {
    posts: Vec<Post>,
}

impl HttpResponse for Response {}

pub fn get_posts() -> impl Future<Item = Json<Response>, Error = Error> + Send + 'static {
    App::with_global(get_conn)
        .and_then(|conn| {
            run_blocking(move || {
                use schema::posts::dsl::*;
                posts.limit(20).load::<Post>(&*conn)
            })
        })
        .map_err(Error::internal_server_error)
        .map(|posts| Json(Response { posts: posts }))
}
