use {
    http::{header, Request, StatusCode},
    tsukuyomi::{
        app::config::prelude::*, //
        chain,
        extractor,
        test::ResponseExt,
        App,
    },
};

#[test]
fn empty_routes() -> tsukuyomi::test::Result<()> {
    let app = App::create(empty())?;
    let mut server = tsukuyomi::test::server(app)?;

    let response = server.perform("/")?;
    assert_eq!(response.status(), StatusCode::NOT_FOUND);

    Ok(())
}

#[test]
fn single_route() -> tsukuyomi::test::Result<()> {
    let app = App::create(
        path!(/"hello") //
            .to(endpoint::any().reply(|| "Tsukuyomi")),
    )?;
    let mut server = tsukuyomi::test::server(app)?;

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
    let app = App::create_with_prefix(
        "/api/v1",
        path!(/"hello") //
            .to(endpoint::any().reply(|| "Tsukuyomi")),
    )?;
    let mut server = tsukuyomi::test::server(app)?;

    assert_eq!(server.perform("/api/v1/hello")?.status(), 200);
    assert_eq!(server.perform("/hello")?.status(), 404);

    Ok(())
}

#[test]
fn post_body() -> tsukuyomi::test::Result<()> {
    let app = App::create(
        path!(/"hello") //
            .to(endpoint::post()
                .extract(tsukuyomi::extractor::body::plain())
                .reply(|body: String| body)),
    )?;
    let mut server = tsukuyomi::test::server(app)?;

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

    let app = App::create(chain![
        path!(/"login") //
            .to(endpoint::any()
                .extract(extractor::guard(move |input| {
                    input.cookies.jar()?.add(
                        Cookie::build("session", "dummy_session_id")
                            .domain("www.example.com")
                            .expires(expires_in)
                            .finish(),
                    );
                    Ok::<_, tsukuyomi::error::Error>(())
                }))
                .reply(|| "Logged in")),
        path!(/"logout") //
            .to(endpoint::any()
                .extract(extractor::guard(|input| {
                    input.cookies.jar()?.remove(Cookie::named("session"));
                    Ok::<_, tsukuyomi::error::Error>(())
                }))
                .reply(|| "Logged out")),
    ])?;
    let mut server = tsukuyomi::test::server(app)?;

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
//     let mut server = App::create(with_modifier(
//         tsukuyomi::handler::modifiers::DefaultOptions::default(),
//         route()
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

    let app = App::create(chain![
        default_handler({
            let marker = marker.clone();
            tsukuyomi::handler::ready(move |_| {
                marker.lock().unwrap().push("F1");
                "f1"
            })
        }),
        mount("/api/v1/").with(chain![
            default_handler({
                let marker = marker.clone();
                tsukuyomi::handler::ready(move |_| {
                    marker.lock().unwrap().push("F2");
                    "f2"
                })
            }),
            path!(/"posts").to(endpoint::post().say("posts")),
            mount("/events").with(
                path!(/"new") //
                    .to(endpoint::post().say("new_event")),
            ),
        ]),
    ])?;

    let mut server = tsukuyomi::test::server(app)?;

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
