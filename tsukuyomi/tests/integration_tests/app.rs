use {
    http::{header, Method, Request, Response, StatusCode},
    tsukuyomi::{
        app::directives::*, //
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
        .with(route("/hello")?.reply(|| "Tsukuyomi"))
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
        .with(route("/hello")?.reply(|| "Tsukuyomi"))
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
            route("/hello")?
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
            route("/login")?
                .extract(extractor::guard(move |input| {
                    input.cookies.jar()?.add(
                        Cookie::build("session", "dummy_session_id")
                            .domain("www.example.com")
                            .expires(expires_in)
                            .finish(),
                    );
                    Ok::<_, tsukuyomi::error::Error>(None)
                })).reply(|| "Logged in"),
        ) //
        .with(
            route("/logout")?
                .extract(extractor::guard(|input| {
                    input.cookies.jar()?.remove(Cookie::named("session"));
                    Ok::<_, tsukuyomi::error::Error>(None)
                })).reply(|| "Logged out"),
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
        .with(route("/path")?.reply(|| "get"))
        .with(route("/path")?.methods("POST")?.reply(|| "post"))
        .build_server()?
        .into_test_server()?;

    let response = server.perform(Request::options("/path"))?;

    assert_eq!(response.status(), StatusCode::OK);
    assert_eq!(response.header(header::ALLOW)?, "GET, POST, OPTIONS");
    assert_eq!(response.header(header::CONTENT_LENGTH)?, "0");

    Ok(())
}

#[test]
fn test_canceled() -> tsukuyomi::test::Result<()> {
    let mut server = App::builder()
        .with(
            route("/")?
                .methods("GET, POST")?
                .extract(tsukuyomi::extractor::guard(
                    |input| -> tsukuyomi::error::Result<_> {
                        if input.request.method() == Method::GET {
                            Ok(None)
                        } else {
                            Ok(Some(Response::new("canceled".into())))
                        }
                    },
                )).reply(|| "passed"),
        ) //
        .build_server()?
        .into_test_server()?;

    let response = server.perform("/")?;
    assert_eq!(response.body().to_utf8()?, "passed");

    let response = server.perform(Request::post("/"))?;
    assert_eq!(response.body().to_utf8()?, "canceled");

    Ok(())
}

#[test]
fn scoped_fallback() -> tsukuyomi::test::Result<()> {
    use std::sync::{Arc, Mutex};
    let marker = Arc::new(Mutex::new(vec![]));

    let mut server = App::builder()
        .fallback({
            let marker = marker.clone();
            move |cx: &tsukuyomi::app::fallback::Context| {
                marker.lock().unwrap().push("F1");
                tsukuyomi::app::fallback::default(cx)
            }
        }) //
        .with(
            mount("/api/v1/")?
                .fallback({
                    let marker = marker.clone();
                    move |cx: &tsukuyomi::app::fallback::Context| {
                        marker.lock().unwrap().push("F2");
                        tsukuyomi::app::fallback::default(cx)
                    }
                }) //
                .with(route("/posts")?.methods("POST")?.say("posts")),
        ) //
        .build_server()?
        .into_test_server()?;

    let _ = server.perform("/")?;
    assert_eq!(&**marker.lock().unwrap(), &*vec!["F1"]);

    marker.lock().unwrap().clear();
    let _ = server.perform("/api/v1/p")?;
    assert_eq!(&**marker.lock().unwrap(), &*vec!["F1"]);

    marker.lock().unwrap().clear();
    let _ = server.perform("/api/v1/posts")?;
    assert_eq!(&**marker.lock().unwrap(), &*vec!["F2"]);

    Ok(())
}
