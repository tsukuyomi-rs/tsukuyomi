extern crate either;
extern crate http;
extern crate serde;
extern crate tsukuyomi;
extern crate tsukuyomi_session;

use tsukuyomi::app::App;
use tsukuyomi::output::Responder;
use tsukuyomi::route;
use tsukuyomi_session::backend::CookieSessionBackend;
use tsukuyomi_session::Session;

use either::Either;
use http::Response;

fn main() {
    let backend = CookieSessionBackend::plain();

    let app = App::builder()
        .modifier(tsukuyomi_session::storage(backend))
        .route({
            route::get("/")
                .with(tsukuyomi_session::extractor())
                .handle(|session: Session| {
                    session.get::<String>("username").map(|username| {
                        if let Some(username) = username {
                            Either::Right(html(format!(
                                "Hello, {}! <br />\n\
                                 <form method=\"post\" action=\"/logout\">\n\
                                 <input type=\"submit\" value=\"Log out\" />\n\
                                 </form>\
                                 ",
                                username
                            )))
                        } else {
                            Either::Left(redirect("/login"))
                        }
                    })
                })
        }).route(
            route::get("/login")
                .with(tsukuyomi_session::extractor())
                .reply(|session: Session| {
                    if session.contains("username") {
                        Either::Left(redirect("/"))
                    } else {
                        Either::Right(html(
                            "login form\n\
                             <form method=\"post\">\n\
                             <input type=\"text\" name=\"username\">\n\
                             <input type=\"submit\">\n\
                             </form>",
                        ))
                    }
                }),
        ).route(
            route::post("/login")
                .with(tsukuyomi::extractor::body::urlencoded())
                .with(tsukuyomi_session::extractor())
                .handle({
                    #[derive(Debug, serde::Deserialize)]
                    struct Form {
                        username: String,
                    }
                    |form: Form, mut session: Session| -> tsukuyomi::error::Result<_> {
                        session.set("username", form.username)?;
                        Ok(redirect("/"))
                    }
                }),
        ).route(
            route::post("/logout")
                .with(tsukuyomi_session::extractor())
                .reply(|mut session: Session| {
                    session.remove("username");
                    redirect("/")
                }),
        ).finish()
        .unwrap();

    tsukuyomi::server::server(app)
        .transport("127.0.0.1:4000")
        .run_forever()
        .unwrap();
}

fn html<D>(body: D) -> Response<D> {
    Response::builder()
        .header("content-type", "text/html; charset=utf-8")
        .body(body)
        .unwrap()
}

fn redirect(location: &str) -> impl Responder {
    Response::builder()
        .status(http::StatusCode::SEE_OTHER)
        .header("location", location)
        .body(())
        .unwrap()
}
