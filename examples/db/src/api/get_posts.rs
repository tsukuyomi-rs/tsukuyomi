use diesel::prelude::*;
use futures::future::poll_fn;
use futures::prelude::*;
use futures::Async;
use tokio_threadpool::blocking;

use tsukuyomi::json::Json;
use tsukuyomi::output::HttpResponse;
use tsukuyomi::Error;

use conn::get_conn;
use model::Post;

#[derive(Debug, Serialize)]
pub struct Response {
    posts: Vec<Post>,
}

impl HttpResponse for Response {}

pub fn get_posts() -> impl Future<Item = Json<Response>, Error = Error> + Send + 'static {
    get_conn()
        .and_then(|conn| {
            let query = {
                use schema::posts::dsl::*;
                posts.limit(20)
            };
            poll_fn(move || {
                try_ready!(blocking(|| query.load::<Post>(&*conn)))
                    .map(Async::Ready)
                    .map_err(Into::into)
            })
        })
        .map_err(Error::internal_server_error)
        .map(|posts| Json(Response { posts: posts }))
}
