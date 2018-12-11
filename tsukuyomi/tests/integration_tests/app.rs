use {
    http::{header, Request, StatusCode},
    tsukuyomi::{
        app::config::prelude::*, chain, extractor, server::Server, test::ResponseExt, App,
    },
};

#[test]
fn empty_routes() -> tsukuyomi::test::Result<()> {
    let mut server = App::configure(()) //
        .map(Server::new)?
        .into_test_server()?;

    let response = server.perform("/")?;
    assert_eq!(response.status(), StatusCode::NOT_FOUND);

    Ok(())
}

#[test]
fn single_route() -> tsukuyomi::test::Result<()> {
    let mut server = App::configure(
        route::root() //
            .segment("hello")?
            .reply(|| "Tsukuyomi"),
    )
    .map(Server::new)?
    .into_test_server()?;

    let response = server.perform("/hello")?;

    assert_eq!(response.status(), StatusCode::OK);
    assert_eq!(
        response.header(header::CONTENT_TYPE)?,
        "text/plain; charset=utf-8"
    );
    assert_eq!(response.header(header::CONTENT_LENGTH)?, "9");
    assert_eq!(*response.body().to_bytes(), b"Tsukuyomi"[..]);

    Ok(())
}

#[test]
fn with_app_prefix() -> tsukuyomi::test::Result<()> {
    let mut server = App::with_prefix(
        "/api/v1",
        route::root() //
            .segment("hello")?
            .reply(|| "Tsukuyomi"),
    )
    .map(Server::new)?
    .into_test_server()?;

    assert_eq!(server.perform("/api/v1/hello")?.status(), 200);
    assert_eq!(server.perform("/hello")?.status(), 404);

    Ok(())
}

#[test]
fn post_body() -> tsukuyomi::test::Result<()> {
    let mut server = App::configure(
        route::root()
            .segment("hello")?
            .allowed_methods("POST")?
            .extract(tsukuyomi::extractor::body::plain())
            .reply(|body: String| body),
    )
    .map(Server::new)?
    .into_test_server()?;

    let response = server.perform(
        Request::post("/hello") //
            .body("Hello, Tsukuyomi."),
    )?;

    assert_eq!(response.status(), StatusCode::OK);
    assert_eq!(
        response.header(header::CONTENT_TYPE)?,
        "text/plain; charset=utf-8"
    );
    assert_eq!(response.header(header::CONTENT_LENGTH)?, "17");
    assert_eq!(*response.body().to_bytes(), b"Hello, Tsukuyomi."[..]);

    Ok(())
}

#[test]
fn cookies() -> tsukuyomi::test::Result<()> {
    use cookie::Cookie;
    use time::Duration;

    let expires_in = time::now() + Duration::days(7);

    let mut server = App::configure(chain![
        route::root()
            .segment("login")?
            .extract(extractor::guard(move |input| {
                input.cookies.jar()?.add(
                    Cookie::build("session", "dummy_session_id")
                        .domain("www.example.com")
                        .expires(expires_in)
                        .finish(),
                );
                Ok::<_, tsukuyomi::error::Error>(())
            }))
            .reply(|| "Logged in"),
        route::root()
            .segment("logout")?
            .extract(extractor::guard(|input| {
                input.cookies.jar()?.remove(Cookie::named("session"));
                Ok::<_, tsukuyomi::error::Error>(())
            }))
            .reply(|| "Logged out"),
    ])
    .map(Server::new)?
    .into_test_server()?;

    let response = server.perform("/login")?;

    let cookie_str = response.header(header::SET_COOKIE)?.to_str()?;
    let cookie = Cookie::parse_encoded(cookie_str)?;
    assert_eq!(cookie.name(), "session");
    assert_eq!(cookie.domain(), Some("www.example.com"));
    assert_eq!(
        cookie.expires().map(|tm| tm.to_timespec().sec),
        Some(expires_in.to_timespec().sec)
    );

    let response = server.perform(Request::get("/logout").header(header::COOKIE, cookie_str))?;

    let cookie_str = response.header(header::SET_COOKIE)?.to_str()?;
    let cookie = Cookie::parse_encoded(cookie_str)?;
    assert_eq!(cookie.name(), "session");
    assert_eq!(cookie.value(), "");
    assert_eq!(cookie.max_age(), Some(Duration::zero()));
    assert!(cookie.expires().map_or(false, |tm| tm < time::now()));

    let response = server.perform("/logout")?;
    assert!(!response.headers().contains_key(header::SET_COOKIE));

    Ok(())
}

// #[test]
// fn default_options() -> tsukuyomi::test::Result<()> {
//     let mut server = App::configure(with_modifier(
//         tsukuyomi::handler::modifiers::DefaultOptions::default(),
//         route::root()
//             .segment("path")?
//             .allowed_methods("GET, POST")?
//             .reply(|| "post"),
//     ))
//     .map(Server::new)?
//     .into_test_server()?;

//     let response = server.perform(Request::options("/path"))?;

//     assert_eq!(response.status(), StatusCode::NO_CONTENT);
//     assert_eq!(response.header(header::ALLOW)?, "GET, POST, OPTIONS");
//     assert_eq!(response.header(header::CONTENT_LENGTH)?, "0");

//     Ok(())
// }

#[test]
fn scoped_fallback() -> tsukuyomi::test::Result<()> {
    use std::sync::{Arc, Mutex};

    let marker = Arc::new(Mutex::new(vec![]));

    let mut server = App::configure(chain![
        default_handler({
            let marker = marker.clone();
            tsukuyomi::handler::ready(move |_| {
                marker.lock().unwrap().push("F1");
                "f1"
            })
        }),
        mount(
            "/api/v1/",
            chain![
                default_handler({
                    let marker = marker.clone();
                    tsukuyomi::handler::ready(move |_| {
                        marker.lock().unwrap().push("F2");
                        "f2"
                    })
                }),
                route::root()
                    .segment("posts")?
                    .allowed_methods("POST")?
                    .say("posts"),
                mount(
                    "/events",
                    route::root()
                        .segment("new")?
                        .allowed_methods("POST")?
                        .say("new_event"),
                ),
            ],
        ),
    ])
    .map(Server::new)?
    .into_test_server()?;

    let _ = server.perform("/")?;
    assert_eq!(&**marker.lock().unwrap(), &*vec!["F1"]);

    marker.lock().unwrap().clear();
    let _ = server.perform("/api/v1/p")?;
    assert_eq!(&**marker.lock().unwrap(), &*vec!["F2"]);

    marker.lock().unwrap().clear();
    let _ = server.perform("/api/v1/posts")?;
    assert!(marker.lock().unwrap().is_empty());

    marker.lock().unwrap().clear();
    let _ = server.perform("/api/v1/events/new")?;
    assert!(marker.lock().unwrap().is_empty());

    Ok(())
}
