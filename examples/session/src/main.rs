extern crate either;
extern crate serde;
extern crate tsukuyomi;
extern crate tsukuyomi_session;

use {
    either::Either,
    tsukuyomi::{
        app::route,
        output::{html, redirect},
    },
    tsukuyomi_session::{backend::CookieBackend, Session, SessionStorage},
};

fn main() -> tsukuyomi::server::Result<()> {
    let backend = CookieBackend::plain();
    let storage = SessionStorage::new(backend);

    tsukuyomi::app!()
        .modifier(storage)
        .route(
            route!("/") //
                .extract(tsukuyomi_session::session())
                .call(|session: Session| -> tsukuyomi::Result<_> {
                    let username = session.get::<String>("username")?;
                    let output = if let Some(username) = username {
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
                    };
                    Ok(session.finish(output))
                }),
        ) //
        .route(
            route!("/login") //
                .extract(tsukuyomi_session::session())
                .reply(|session: Session| {
                    let output = if session.contains("username") {
                        Either::Left(redirect::to("/"))
                    } else {
                        Either::Right(html(
                            "login form\n\
                             <form method=\"post\">\n\
                             <input type=\"text\" name=\"username\">\n\
                             <input type=\"submit\">\n\
                             </form>",
                        ))
                    };
                    session.finish(output)
                }),
        ) //
        .route({
            #[derive(Debug, serde::Deserialize)]
            struct Form {
                username: String,
            }
            route!("/login")
                .extract(tsukuyomi::extractor::body::urlencoded())
                .extract(tsukuyomi_session::session())
                .call(
                    |form: Form, mut session: Session| -> tsukuyomi::error::Result<_> {
                        session.set("username", form.username)?;
                        Ok(session.finish(redirect::to("/")))
                    },
                )
        }) //
        .route(
            route!("/logout") //
                .extract(tsukuyomi_session::session())
                .reply(|mut session: Session| {
                    session.remove("username");
                    session.finish(redirect::to("/"))
                }),
        ) //
        .build_server()?
        .run()
}
