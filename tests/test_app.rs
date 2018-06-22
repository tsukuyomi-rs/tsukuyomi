extern crate futures;
extern crate http;
extern crate tsukuyomi;

use tsukuyomi::local::LocalServer;
use tsukuyomi::{App, Input};

use futures::future::lazy;
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

#[test]
fn test_case3_post_body() {
    let app = App::builder()
        .mount("/", |m| {
            m.post("/hello").handle_async(|| {
                lazy(|| {
                    let read_all = Input::with_get(|input| input.body_mut().read_all());
                    read_all.convert_to::<String>()
                })
            });
        })
        .finish()
        .unwrap();
    let mut server = LocalServer::new(app).unwrap();

    let response = server
        .client()
        .post("/hello")
        .body("Hello, Tsukuyomi.")
        .execute()
        .unwrap();

    assert_eq!(response.status(), StatusCode::OK);
    assert_eq!(
        response.headers().get(header::CONTENT_TYPE).map(|v| v.as_bytes()),
        Some(&b"text/plain; charset=utf-8"[..])
    );
    assert_eq!(
        response.headers().get(header::CONTENT_LENGTH).map(|v| v.as_bytes()),
        Some(&b"17"[..])
    );
    assert_eq!(*response.body().to_bytes(), b"Hello, Tsukuyomi."[..]);
}

#[test]
fn test_case4_modifier() {
    use std::sync::atomic::{AtomicBool, Ordering};
    use std::sync::Arc;
    use tsukuyomi::modifier::{AfterHandle, BeforeHandle, Modifier};
    use tsukuyomi::output::Output;

    struct MyModifier(Arc<AtomicBool>, Arc<AtomicBool>);

    impl Modifier for MyModifier {
        fn before_handle(&self, _: &mut Input) -> BeforeHandle {
            self.0.store(true, Ordering::SeqCst);
            BeforeHandle::ok()
        }

        fn after_handle(&self, _: &mut Input, output: Output) -> AfterHandle {
            self.1.store(true, Ordering::SeqCst);
            AfterHandle::ok(output)
        }
    }

    let flag1 = Arc::new(AtomicBool::new(false));
    let flag2 = Arc::new(AtomicBool::new(false));

    let app = App::builder()
        .modifier(MyModifier(flag1.clone(), flag2.clone()))
        .mount("/", |m| {
            m.get("/").handle(|_| "dummy");
        })
        .finish()
        .unwrap();

    let mut server = LocalServer::new(app).unwrap();

    let _ = server.client().get("/").execute().unwrap();

    assert_eq!(flag1.load(Ordering::SeqCst), true);
    assert_eq!(flag2.load(Ordering::SeqCst), true);
}
