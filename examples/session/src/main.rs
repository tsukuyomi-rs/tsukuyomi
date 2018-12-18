use {
    either::Either,
    std::sync::Arc,
    tsukuyomi::{
        config::prelude::*, //
        extractor,
        output::{html, redirect},
        App,
        Server,
    },
    tsukuyomi_session::{
        backend::CookieBackend, //
        session,
        Session,
    },
};

fn main() -> tsukuyomi::server::Result<()> {
    let backend = CookieBackend::plain();
    let session = Arc::new(session(backend));

    App::create(chain![
        path!("/") //
            .to(endpoint::get() //
                .extract(session.clone())
                .call_async(|session: Session<_>| -> tsukuyomi::Result<_> {
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
                })),
        path!("/login") //
            .to(chain![
                endpoint::get() //
                    .extract(session.clone())
                    .call(|session: Session<_>| {
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
                endpoint::post()
                    .extract(session.clone())
                    .extract(extractor::body::urlencoded())
                    .call_async({
                        #[derive(Debug, serde::Deserialize)]
                        struct Form {
                            username: String,
                        }
                        |mut session: Session<_>, form: Form| -> tsukuyomi::Result<_> {
                            session.set("username", form.username)?;
                            Ok(session.finish(redirect::to("/")))
                        }
                    }),
            ]),
        path!("/logout") //
            .to(endpoint::get()
                .extract(session)
                .call(|mut session: Session<_>| {
                    session.remove("username");
                    session.finish(redirect::to("/"))
                }))
    ])
    .map(Server::new)?
    .run()
}
