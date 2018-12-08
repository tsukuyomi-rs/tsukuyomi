use {
    http::{header, Request, StatusCode},
    tsukuyomi::{
        app::{fallback, mount, route, App},
        extractor,
        test::ResponseExt,
    },
};

#[test]
fn empty_routes() -> tsukuyomi::test::Result<()> {
    let mut server = App::builder() //
        .build_server()?
        .into_test_server()?;

    let response = server.perform("/")?;
    assert_eq!(response.status(), StatusCode::NOT_FOUND);

    Ok(())
}

#[test]
fn single_route() -> tsukuyomi::test::Result<()> {
    let mut server = App::builder()
        .with(route::root().segment("hello")?.reply(|| "Tsukuyomi"))
        .build_server()?
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
    let mut server = App::with_prefix("/api/v1")?
        .with(route::root().segment("hello")?.reply(|| "Tsukuyomi"))
        .build_server()?
        .into_test_server()?;

    assert_eq!(server.perform("/api/v1/hello")?.status(), 200);
    assert_eq!(server.perform("/hello")?.status(), 404);

    Ok(())
}

#[test]
fn post_body() -> tsukuyomi::test::Result<()> {
    let mut server = App::builder()
        .with(
            route::root()
                .segment("hello")?
                .methods("POST")?
                .extract(tsukuyomi::extractor::body::plain())
                .reply(|body: String| body),
        ) //
        .build_server()?
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

    let mut server = App::builder()
        .with(
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
        ) //
        .with(
            route::root()
                .segment("logout")?
                .extract(extractor::guard(|input| {
                    input.cookies.jar()?.remove(Cookie::named("session"));
                    Ok::<_, tsukuyomi::error::Error>(())
                }))
                .reply(|| "Logged out"),
        ) //
        .build_server()?
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

#[test]
fn default_options() -> tsukuyomi::test::Result<()> {
    let mut server = App::builder()
        .with(route::root().segment("path")?.reply(|| "get"))
        .with(
            route::root()
                .segment("path")?
                .methods("POST")?
                .reply(|| "post"),
        )
        .build_server()?
        .into_test_server()?;

    let response = server.perform(Request::options("/path"))?;

    assert_eq!(response.status(), StatusCode::OK);
    assert_eq!(response.header(header::ALLOW)?, "GET, POST, OPTIONS");
    assert_eq!(response.header(header::CONTENT_LENGTH)?, "0");

    Ok(())
}

#[test]
fn scoped_fallback() -> tsukuyomi::test::Result<()> {
    use std::sync::{Arc, Mutex};
    let marker = Arc::new(Mutex::new(vec![]));

    let mut server = App::builder()
        .with(fallback({
            let marker = marker.clone();
            move |cx: &mut fallback::Context<'_>| {
                marker.lock().unwrap().push("F1");
                fallback::default(cx)
            }
        })) //
        .with(
            mount("/api/v1/")?
                .with(fallback({
                    let marker = marker.clone();
                    move |cx: &mut fallback::Context<'_>| {
                        marker.lock().unwrap().push("F2");
                        fallback::default(cx)
                    }
                })) //
                .with(
                    route::root()
                        .segment("posts")?
                        .methods("POST")?
                        .say("posts"),
                )
                .with(
                    mount("/events")?.with(
                        route::root()
                            .segment("new")?
                            .methods("POST")?
                            .say("new_event"),
                    ),
                ),
        ) //
        .build_server()?
        .into_test_server()?;

    let _ = server.perform("/")?;
    assert_eq!(&**marker.lock().unwrap(), &*vec!["F1"]);

    marker.lock().unwrap().clear();
    let _ = server.perform("/api/v1/p")?;
    assert_eq!(&**marker.lock().unwrap(), &*vec!["F2"]);

    marker.lock().unwrap().clear();
    let _ = server.perform("/api/v1/posts")?;
    assert_eq!(&**marker.lock().unwrap(), &*vec!["F2"]);

    marker.lock().unwrap().clear();
    let _ = server.perform("/api/v1/events/new")?;
    assert_eq!(&**marker.lock().unwrap(), &*vec!["F2"]);

    Ok(())
}
