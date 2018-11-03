use diesel::prelude::*;
use failure::SyncFailure;
use futures::prelude::*;
use serde_qs;

use tsukuyomi::json::{HttpResponse, Json};
use tsukuyomi::{AsyncResponder, Error, Input};

use conn::{get_conn, run_blocking};
use model::Post;

#[derive(Debug, Deserialize)]
pub struct Param {
    #[serde(default)]
    count: i64,
}

impl Default for Param {
    fn default() -> Self {
        Param { count: 20 }
    }
}

#[derive(Debug, Serialize)]
pub struct Response(Vec<Post>);

impl HttpResponse for Response {}

pub fn get_posts(input: &mut Input) -> impl AsyncResponder<Output = Json<Response>> {
    let param = input
        .uri()
        .query()
        .map(|query| serde_qs::from_str::<Param>(query).map_err(|err| Error::bad_request(SyncFailure::new(err))))
        .unwrap_or_else(|| Ok(Default::default()));

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
                use schema::posts::dsl::*;
                posts.limit(param.count).load::<Post>(&*conn)
            }).map_err(Error::internal_server_error)
        })
        .map(|posts| Json(Response(posts)))
}
