extern crate http;
extern crate serde;
extern crate tsukuyomi;
extern crate tsukuyomi_cors;

use http::{Method, Uri};
use tsukuyomi_cors::CORS;

#[derive(Debug, serde::Deserialize, serde::Serialize, tsukuyomi::Responder)]
#[responder(respond_to = "tsukuyomi::output::responder::json")]
struct UserInfo {
    username: String,
    email: String,
    password: String,
    confirm_password: String,
}

fn main() -> tsukuyomi::server::Result<()> {
    let cors = CORS::builder()
        .allow_origin(Uri::from_static("http://127.0.0.1:5000"))
        .allow_methods(vec![Method::GET, Method::POST])
        .allow_header(http::header::CONTENT_TYPE)
        .max_age(std::time::Duration::from_secs(3600))
        .build();

    tsukuyomi::app!()
        .with(cors)
        .route(
            tsukuyomi::route!("/user/info", method = POST) //
                .with(tsukuyomi::extractor::body::json())
                .handle(|info: UserInfo| -> tsukuyomi::Result<_> {
                    if info.password != info.confirm_password {
                        return Err(tsukuyomi::error::bad_request(
                            "the field confirm_password is not matched to password.",
                        ));
                    }
                    Ok(info)
                }),
        ) //
        .build_server()?
        .bind(std::net::SocketAddr::from(([127, 0, 0, 1], 4000)))
        .run()
}
