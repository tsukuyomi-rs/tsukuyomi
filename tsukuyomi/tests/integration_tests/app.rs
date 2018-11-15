use tsukuyomi::app::route;
use tsukuyomi::extractor;
use tsukuyomi::test::ResponseExt;

use http::{header, Method, Request, Response, StatusCode};

#[test]
fn empty_routes() -> tsukuyomi::test::Result<()> {
    let mut server = tsukuyomi::app() //
        .build_server()?
        .into_test_server()?;

    let response = server.perform("/")?;
    assert_eq!(response.status(), StatusCode::NOT_FOUND);

    Ok(())
}

#[test]
fn single_route() -> tsukuyomi::test::Result<()> {
    let mut server = tsukuyomi::app()
        .route(route!("/hello").reply(|| "Tsukuyomi"))
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
fn post_body() -> tsukuyomi::test::Result<()> {
    let mut server = tsukuyomi::app()
        .route(
            route!("/hello", method = POST)
                .with(tsukuyomi::extractor::body::plain())
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

    let mut server = tsukuyomi::app()
        .route(
            route!("/login")
                .with(extractor::guard(move |input| {
                    let mut cookies = input.cookies()?;
                    cookies.add(
                        Cookie::build("session", "dummy_session_id")
                            .domain("www.example.com")
                            .expires(expires_in)
                            .finish(),
                    );
                    Ok::<_, tsukuyomi::error::Error>(None)
                })).reply(|| "Logged in"),
        ) //
        .route(
            route!("/logout")
                .with(extractor::guard(|input| {
                    let mut cookies = input.cookies()?;
                    cookies.remove(Cookie::named("session"));
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
    let mut server = tsukuyomi::app()
        .route(route!("/path").reply(|| "get"))
        .route(route!("/path", method = POST).reply(|| "post"))
        .build_server()?
        .into_test_server()?;

    let response = server.perform(Request::options("/path"))?;

    assert_eq!(response.status(), StatusCode::OK);
    assert_eq!(response.header(header::ALLOW)?, "GET, POST, OPTIONS");
    assert_eq!(response.header(header::CONTENT_LENGTH)?, "0");

    Ok(())
}

#[test]
fn test_case_5_disable_default_options() -> tsukuyomi::test::Result<()> {
    let mut server = tsukuyomi::app()
        .global(tsukuyomi::app::global().fallback_options(false)) //
        .route(route!("/path").reply(|| "get"))
        .route(route!("/path", method = POST).reply(|| "post"))
        .build_server()?
        .into_test_server()?;

    let response = server.perform(Request::options("/path"))?;
    assert_eq!(response.status(), StatusCode::METHOD_NOT_ALLOWED);

    Ok(())
}

#[test]
fn test_canceled() -> tsukuyomi::test::Result<()> {
    let mut server = tsukuyomi::app()
        .route(
            route!("/", methods = [GET, POST])
                .with(tsukuyomi::extractor::guard(
                    |input| -> tsukuyomi::error::Result<_> {
                        if input.method() == Method::GET {
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
