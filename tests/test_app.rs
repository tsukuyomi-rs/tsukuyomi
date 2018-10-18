extern crate cookie;
extern crate http;
extern crate time;
extern crate tsukuyomi;
extern crate tsukuyomi_server;

use tsukuyomi::app::App;
use tsukuyomi::handler;
use tsukuyomi_server::local::LocalServer;

use http::{header, Method, Request, StatusCode};

#[test]
fn test_case1_empty_routes() {
    let app = App::builder().finish().unwrap();
    let mut server = LocalServer::new(app).unwrap();

    let response = server
        .client()
        .unwrap()
        .perform(Request::get("/").body(Default::default()).unwrap())
        .unwrap();

    assert_eq!(response.status(), StatusCode::NOT_FOUND);
}

#[test]
fn test_case2_single_route() {
    let app = App::builder()
        .mount("/", |m| {
            m.route(("/hello", handler::wrap_ready(|_| "Tsukuyomi")));
        }).finish()
        .unwrap();
    let mut server = LocalServer::new(app).unwrap();

    let response = server
        .client()
        .unwrap()
        .perform(Request::get("/hello").body(Default::default()).unwrap())
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
        Some(&b"9"[..])
    );
    assert_eq!(*response.body().to_bytes(), b"Tsukuyomi"[..]);
}

#[test]
fn test_case3_post_body() {
    let app = App::builder()
        .route((
            "/hello",
            Method::POST,
            handler::wrap_async(|input| input.body_mut().read_all().convert_to::<String>()),
        )).finish()
        .unwrap();
    let mut server = LocalServer::new(app).unwrap();

    let response = server
        .client()
        .unwrap()
        .perform(
            Request::post("/hello")
                .body("Hello, Tsukuyomi.".into())
                .unwrap(),
        ).unwrap();

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
fn test_case4_cookie() {
    use cookie::Cookie;
    use time::Duration;

    let expires_in = time::now() + Duration::days(7);

    let app = App::builder()
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
        )).finish()
        .unwrap();

    let mut server = LocalServer::new(app).unwrap();

    let response = server
        .client()
        .unwrap()
        .perform(Request::get("/login").body(Default::default()).unwrap())
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
    assert_eq!(cookie.domain(), Some("www.example.com"));
    assert_eq!(
        cookie.expires().map(|tm| tm.to_timespec().sec),
        Some(expires_in.to_timespec().sec)
    );

    let response = server
        .client()
        .unwrap()
        .perform(
            Request::get("/logout")
                .header(header::COOKIE, cookie_str)
                .body(Default::default())
                .unwrap(),
        ).unwrap();
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

    let response = server
        .client()
        .unwrap()
        .perform(Request::get("/logout").body(Default::default()).unwrap())
        .unwrap();
    assert!(!response.headers().contains_key(header::SET_COOKIE));
}

#[test]
fn test_case_5_default_options() {
    let app = App::builder()
        .route(("/path", Method::GET, handler::wrap_ready(|_| "get")))
        .route(("/path", Method::POST, handler::wrap_ready(|_| "post")))
        .finish()
        .unwrap();
    let mut server = LocalServer::new(app).unwrap();

    let response = server
        .client()
        .unwrap()
        .perform(Request::options("/path").body(Default::default()).unwrap())
        .unwrap();

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
    let app = App::builder()
        .route(("/path", Method::GET, handler::wrap_ready(|_| "get")))
        .route(("/path", Method::POST, handler::wrap_ready(|_| "post")))
        .default_options(false)
        .finish()
        .unwrap();
    let mut server = LocalServer::new(app).unwrap();

    let response = server
        .client()
        .unwrap()
        .perform(Request::options("/path").body(Default::default()).unwrap())
        .unwrap();
    assert_eq!(response.status(), StatusCode::METHOD_NOT_ALLOWED);
}
