use {
    serde::{Deserialize, Serialize},
    tsukuyomi::{
        config::prelude::*, //
        extractor,
        output::IntoResponse,
        App,
        Server,
    },
    tsukuyomi_cors::CORS,
};

#[derive(Debug, Deserialize, Serialize, IntoResponse)]
#[response(with = "tsukuyomi::output::into_response::json")]
struct UserInfo {
    username: String,
    email: String,
    password: String,
    confirm_password: String,
}

fn main() -> tsukuyomi::server::Result<()> {
    let cors = CORS::builder()
        .allow_origin("http://127.0.0.1:5000")?
        .allow_methods(vec!["GET", "POST"])?
        .allow_header("content-type")?
        .max_age(std::time::Duration::from_secs(3600))
        .build();

    App::create(chain![
        path!("*").to(cors.clone()), // handle OPTIONS *
        path!("/user/info") //
            .to(endpoint::post() //
                .extract(extractor::body::json())
                .call(|info: UserInfo| -> tsukuyomi::Result<_> {
                    if info.password != info.confirm_password {
                        return Err(tsukuyomi::error::bad_request(
                            "the field confirm_password is not matched to password.",
                        ));
                    }
                    Ok(info)
                },))
            .modify(cors), // <-- handle CORS simple/preflight request to `/user/info`
    ]) //
    .map(Server::new)?
    .bind(std::net::SocketAddr::from(([127, 0, 0, 1], 4000)))
    .run()
}
