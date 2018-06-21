extern crate http;
extern crate tsukuyomi;

use tsukuyomi::local::LocalServer;
use tsukuyomi::App;

use http::{header, StatusCode};

#[test]
fn test_case1_empty_routes() {
    let app = App::builder().finish().unwrap();
    let mut server = LocalServer::new(app).unwrap();

    let response = server.client().get("/").execute().unwrap();

    assert_eq!(response.status(), StatusCode::NOT_FOUND);
}

#[test]
fn test_case2_single_route() {
    let app = App::builder()
        .mount("/", |m| {
            m.get("/hello").handle(|_| "Tsukuyomi");
        })
        .finish()
        .unwrap();
    let mut server = LocalServer::new(app).unwrap();

    let response = server.client().get("/hello").execute().unwrap();

    assert_eq!(response.status(), StatusCode::OK);
    assert_eq!(
        response.headers().get(header::CONTENT_TYPE).map(|v| v.as_bytes()),
        Some(&b"text/plain; charset=utf-8"[..])
    );
    assert_eq!(
        response.headers().get(header::CONTENT_LENGTH).map(|v| v.as_bytes()),
        Some(&b"9"[..])
    );
    assert_eq!(*response.body().to_bytes(), b"Tsukuyomi"[..]);
}
