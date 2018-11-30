extern crate either;
extern crate serde;
extern crate tsukuyomi;
extern crate tsukuyomi_session;

use {
    either::Either,
    tsukuyomi::{
        app::directives::*,
        extractor,
        output::{html, redirect},
    },
    tsukuyomi_session::{backend::CookieBackend, session, Session, SessionStorage},
};

fn main() -> tsukuyomi::server::Result<()> {
    let backend = CookieBackend::plain();
    let storage = SessionStorage::new(backend);

    App::builder()
        .with(modifier(storage))
        .with(
            route!("/") //
                .extract(session())
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
        .with(
            route!("/login") //
                .extract(session())
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
        .with({
            #[derive(Debug, serde::Deserialize)]
            struct Form {
                username: String,
            }
            route!("/login")
                .extract(extractor::body::urlencoded())
                .extract(session())
                .call(
                    |form: Form, mut session: Session| -> tsukuyomi::error::Result<_> {
                        session.set("username", form.username)?;
                        Ok(session.finish(redirect::to("/")))
                    },
                )
        }) //
        .with(
            route!("/logout") //
                .extract(session())
                .reply(|mut session: Session| {
                    session.remove("username");
                    session.finish(redirect::to("/"))
                }),
        ) //
        .build_server()?
        .run()
}
