use diesel;
use diesel::prelude::*;
use futures::prelude::*;
use http::StatusCode;

use tsukuyomi::json::Json;
use tsukuyomi::output::HttpResponse;
use tsukuyomi::{Error, Input};

use conn::{get_conn, run_blocking};
use model::NewPost;
use schema::posts;

#[derive(Debug, Deserialize)]
pub struct Param {
    title: String,
    body: String,
}

#[derive(Debug, Serialize)]
pub struct Response {
    // TODO: return created Post
    #[serde(skip)]
    _priv: (),
}

impl HttpResponse for Response {
    fn status_code(&self) -> StatusCode {
        StatusCode::CREATED
    }
}

pub fn create_post(input: &mut Input) -> impl Future<Item = Json<Response>, Error = Error> + Send + 'static {
    let param = input
        .body_mut()
        .read_all()
        .convert_to::<Json<Param>>()
        .map(Json::into_inner);
    let conn = get_conn(input.get()).map_err(Error::internal_server_error);

    (param, conn)
        .into_future()
        .and_then(|(param, conn)| {
            run_blocking(move || {
                let new_post = NewPost {
                    title: &param.title,
                    body: &param.body,
                };
                diesel::insert_into(posts::table).values(&new_post).execute(&*conn)
            }).map_err(Error::internal_server_error)
        })
        .map(|_| Json(Response { _priv: () }))
}
