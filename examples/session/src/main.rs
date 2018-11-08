extern crate either;
extern crate serde;
extern crate tsukuyomi;
extern crate tsukuyomi_session;

use tsukuyomi::output::{html, redirect};
use tsukuyomi::route;
use tsukuyomi_session::backend::CookieSessionBackend;
use tsukuyomi_session::Session;

use either::Either;

fn main() {
    let backend = CookieSessionBackend::plain();

    let app = tsukuyomi::app(|scope| {
        scope.modifier(tsukuyomi_session::storage(backend));

        scope.route(route::get("/").with(tsukuyomi_session::extractor()).handle(
            |session: Session| {
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
                        Either::Left(redirect::to("/login"))
                    }
                })
            },
        ));

        scope.route(
            route::get("/login")
                .with(tsukuyomi_session::extractor())
                .reply(|session: Session| {
                    if session.contains("username") {
                        Either::Left(redirect::to("/"))
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
        );

        scope.route(
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
                        Ok(redirect::to("/"))
                    }
                }),
        );

        scope.route(
            route::post("/logout")
                .with(tsukuyomi_session::extractor())
                .reply(|mut session: Session| {
                    session.remove("username");
                    redirect::to("/")
                }),
        );
    }).unwrap();

    tsukuyomi::server(app)
        .bind("127.0.0.1:4000")
        .run_forever()
        .unwrap();
}
