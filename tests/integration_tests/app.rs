use tsukuyomi::app::App;
use tsukuyomi::handler;
use tsukuyomi::input::body::Plain;

use futures::prelude::*;
use http::{header, Method, Request, StatusCode};

use super::util::{local_server, LocalServerExt};

#[test]
fn empty_routes() {
    let mut server = local_server(App::builder());

    let response = server.perform(Request::get("/")).unwrap();
    assert_eq!(response.status(), StatusCode::NOT_FOUND);
}

#[test]
fn single_route() {
    let mut server = local_server(App::builder().mount("/", |m| {
        m.route(("/hello", handler::wrap_ready(|_| "Tsukuyomi")));
    }));

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
    let mut server = local_server(App::builder().route((
        "/hello",
        Method::POST,
        handler::wrap_async(|input| input.extract::<Plain>().map(Plain::into_inner)),
    )));

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

    let mut server = local_server(
        App::builder()
            .route((
                "/login",
                handler::wrap_ready({
                    move |input| -> tsukuyomi::error::Result<_> {
                        #[cfg_attr(rustfmt, rustfmt_skip)]
                    let cookie = Cookie::build("session", "dummy_session_id")
                        .domain("www.example.com")
                        .expires(expires_in)
                        .finish();
                        input.cookies()?.add(cookie);
                        Ok("Logged in")
                    }
                }),
            )).route((
                "/logout",
                handler::wrap_ready(move |input| -> tsukuyomi::error::Result<_> {
                    input.cookies()?.remove(Cookie::named("session"));
                    Ok("Logged out")
                }),
            )),
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
    let mut server = local_server(
        App::builder()
            .route(("/path", Method::GET, handler::wrap_ready(|_| "get")))
            .route(("/path", Method::POST, handler::wrap_ready(|_| "post"))),
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
    let mut server = local_server(
        App::builder()
            .route(("/path", Method::GET, handler::wrap_ready(|_| "get")))
            .route(("/path", Method::POST, handler::wrap_ready(|_| "post")))
            .default_options(false),
    );

    let response = server.perform(Request::options("/path")).unwrap();
    assert_eq!(response.status(), StatusCode::METHOD_NOT_ALLOWED);
}
