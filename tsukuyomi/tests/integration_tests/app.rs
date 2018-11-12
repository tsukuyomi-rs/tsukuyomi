use tsukuyomi::app::App;
use tsukuyomi::extractor;
use tsukuyomi::route;
use tsukuyomi::test::test_server;

use http::{header, Request, StatusCode};

#[test]
fn empty_routes() {
    let mut server = test_server(
        App::builder() //
            .finish()
            .unwrap(),
    );

    let response = server.perform(Request::get("/")).unwrap();
    assert_eq!(response.status(), StatusCode::NOT_FOUND);
}

#[test]
fn single_route() {
    let mut server = test_server(
        App::builder()
            .route(route!("/hello").reply(|| "Tsukuyomi"))
            .finish()
            .unwrap(),
    );

    let response = server.perform(Request::get("/hello")).unwrap();

    assert_eq!(response.status(), StatusCode::OK);
    assert_eq!(
        response
            .headers()
            .get(header::CONTENT_TYPE)
            .map(|v| v.as_bytes()),
        Some(&b"text/plain; charset=utf-8"[..])
    );
    assert_eq!(
        response
            .headers()
            .get(header::CONTENT_LENGTH)
            .map(|v| v.as_bytes()),
        Some(&b"9"[..])
    );
    assert_eq!(*response.body().to_bytes(), b"Tsukuyomi"[..]);
}

#[test]
fn post_body() {
    let mut server = test_server(
        App::builder()
            .route(
                route!("/hello", method = POST)
                    .with(tsukuyomi::extractor::body::plain())
                    .reply(|body: String| body),
            ) //
            .finish()
            .unwrap(),
    );

    let response = server
        .perform(Request::post("/hello").body("Hello, Tsukuyomi."))
        .unwrap();

    assert_eq!(response.status(), StatusCode::OK);
    assert_eq!(
        response
            .headers()
            .get(header::CONTENT_TYPE)
            .map(|v| v.as_bytes()),
        Some(&b"text/plain; charset=utf-8"[..])
    );
    assert_eq!(
        response
            .headers()
            .get(header::CONTENT_LENGTH)
            .map(|v| v.as_bytes()),
        Some(&b"17"[..])
    );
    assert_eq!(*response.body().to_bytes(), b"Hello, Tsukuyomi."[..]);
}

#[test]
fn cookies() {
    use cookie::Cookie;
    use time::Duration;

    let expires_in = time::now() + Duration::days(7);

    let mut server = test_server(
        App::builder()
            .route(
                route!("/login")
                    .with(extractor::validate(move |input| {
                        let cookie = Cookie::build("session", "dummy_session_id")
                            .domain("www.example.com")
                            .expires(expires_in)
                            .finish();
                        input.cookies().map(|mut cookies| {
                            cookies.add(cookie);
                        })
                    })).reply(|| "Logged in"),
            ) //
            .route(
                route!("/logout")
                    .with(extractor::validate(|input| {
                        input.cookies().map(|mut cookies| {
                            cookies.remove(Cookie::named("session"));
                        })
                    })).reply(|| "Logged out"),
            ) //
            .finish()
            .unwrap(),
    );

    let response = server.perform(Request::get("/login")).unwrap();
    assert!(response.headers().contains_key(header::SET_COOKIE));

    let cookie_str = response
        .headers()
        .get(header::SET_COOKIE)
        .unwrap()
        .to_str()
        .unwrap();
    let cookie = Cookie::parse_encoded(cookie_str).unwrap();
    assert_eq!(cookie.name(), "session");
    assert_eq!(cookie.domain(), Some("www.example.com"));
    assert_eq!(
        cookie.expires().map(|tm| tm.to_timespec().sec),
        Some(expires_in.to_timespec().sec)
    );

    let response = server
        .perform(Request::get("/logout").header(header::COOKIE, cookie_str))
        .unwrap();
    assert!(response.headers().contains_key(header::SET_COOKIE));

    let cookie_str = response
        .headers()
        .get(header::SET_COOKIE)
        .unwrap()
        .to_str()
        .unwrap();
    let cookie = Cookie::parse_encoded(cookie_str).unwrap();
    assert_eq!(cookie.name(), "session");
    assert_eq!(cookie.value(), "");
    assert_eq!(cookie.max_age(), Some(Duration::zero()));
    assert!(cookie.expires().map_or(false, |tm| tm < time::now()));

    let response = server.perform(Request::get("/logout")).unwrap();
    assert!(!response.headers().contains_key(header::SET_COOKIE));
}

#[test]
fn default_options() {
    let mut server = test_server(
        App::builder()
            .route(route!("/path").reply(|| "get"))
            .route(route!("/path", method = POST).reply(|| "post"))
            .finish()
            .unwrap(),
    );

    let response = server.perform(Request::options("/path")).unwrap();

    assert_eq!(response.status(), StatusCode::OK);
    assert_eq!(
        response.headers().get(header::ALLOW).map(|v| v.as_bytes()),
        Some(&b"GET, POST, OPTIONS"[..])
    );
    assert_eq!(
        response
            .headers()
            .get(header::CONTENT_LENGTH)
            .map(|v| v.as_bytes()),
        Some(&b"0"[..])
    );
}

#[test]
fn test_case_5_disable_default_options() {
    let mut server = test_server(
        App::builder()
            .config(|g| {
                g.fallback_options(false);
            }) //
            .route(route!("/path").reply(|| "get"))
            .route(route!("/path", method = POST).reply(|| "post"))
            .finish()
            .unwrap(),
    );

    let response = server.perform(Request::options("/path")).unwrap();
    assert_eq!(response.status(), StatusCode::METHOD_NOT_ALLOWED);
}
