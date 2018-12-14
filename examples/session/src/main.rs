use {
    either::Either,
    std::sync::Arc,
    tsukuyomi::{
        app::config::prelude::*,
        chain, extractor,
        output::{html, redirect, IntoResponse},
        server::Server,
        App,
    },
    tsukuyomi_session::{
        backend::CookieBackend, //
        session,
        Session,
    },
};

fn either<L, R>(either: Either<L, R>) -> impl IntoResponse
where
    L: IntoResponse,
    R: IntoResponse,
{
    tsukuyomi::output::into_response(move |input| match either {
        Either::Left(l) => l
            .into_response(input)
            .map(|res| res.map(Into::into))
            .map_err(Into::into),
        Either::Right(r) => r
            .into_response(input)
            .map(|res| res.map(Into::into))
            .map_err(Into::into),
    })
}

fn main() -> tsukuyomi::server::Result<()> {
    let backend = CookieBackend::plain();
    let session = Arc::new(session(backend));

    App::create(chain![
        path!(/)
            .extract(session.clone())
            .to(
                endpoint::get().call_async(|session: Session<_>| -> tsukuyomi::Result<_> {
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
                    Ok(session.finish(either(output)))
                }),
            ),
        path!(/"login") //
            .extract(session.clone())
            .to(chain![
                endpoint::get().call(|session: Session<_>| {
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
                    session.finish(either(output))
                }),
                endpoint::post()
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
        path!(/"logout") //
            .extract(session)
            .to(endpoint::get().call(|mut session: Session<_>| {
                session.remove("username");
                session.finish(redirect::to("/"))
            }))
    ])
    .map(Server::new)?
    .run()
}
