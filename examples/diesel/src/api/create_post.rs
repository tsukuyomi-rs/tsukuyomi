use diesel;
use diesel::prelude::*;
use futures::prelude::*;
use http::StatusCode;

use tsukuyomi::json::{HttpResponse, Json};
use tsukuyomi::{AsyncResponder, Error, Input};

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

pub fn create_post(input: &mut Input) -> impl AsyncResponder<Output = Json<Response>> {
    let param = input
        .body_mut()
        .read_all()
        .convert_to::<Json<Param>>()
        .map(Json::into_inner);

    let conn = input
        .get()
        .ok_or_else(|| Error::internal_server_error(format_err!("missing DB pool")))
        .map(|pool| get_conn(pool))
        .into_future()
        .and_then(|conn| conn.map_err(Error::internal_server_error));

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
