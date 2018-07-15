use super::router::{Recognize, RecognizeErrorKind};
use super::*;
use handler::Handle;
use http::{Method, Response};
use input::Input;

fn dummy_handler(_: &mut Input) -> Handle {
    Handle::ok(Response::new(()).into())
}

#[test]
fn empty() {
    let app = App::builder().finish().unwrap();
    assert_matches!(
        app.router().recognize("/", &Method::GET),
        Err(RecognizeErrorKind::NotFound)
    );
}

#[test]
fn route_single_method() {
    let app = App::builder().route(("/", dummy_handler)).finish().unwrap();

    assert_matches!(
        app.router().recognize("/", &Method::GET),
        Ok(Recognize { endpoint_id: 0, .. })
    );

    assert_matches!(
        app.router().recognize("/path/to", &Method::GET),
        Err(RecognizeErrorKind::NotFound)
    );
    assert_matches!(
        app.router().recognize("/", &Method::POST),
        Err(RecognizeErrorKind::MethodNotAllowed)
    );
}

#[test]
fn route_multiple_method() {
    let app = App::builder()
        .route(("/", dummy_handler))
        .route(("/", Method::POST, dummy_handler))
        .finish()
        .unwrap();

    assert_matches!(
        app.router().recognize("/", &Method::GET),
        Ok(Recognize { endpoint_id: 0, .. })
    );
    assert_matches!(
        app.router().recognize("/", &Method::POST),
        Ok(Recognize { endpoint_id: 1, .. })
    );

    assert_matches!(
        app.router().recognize("/", &Method::PUT),
        Err(RecognizeErrorKind::MethodNotAllowed)
    );
}

#[test]
fn route_fallback_head_enabled() {
    let app = App::builder().route(("/", dummy_handler)).finish().unwrap();

    assert_matches!(
        app.router().recognize("/", &Method::HEAD),
        Ok(Recognize { endpoint_id: 0, .. })
    );
}

#[test]
fn route_fallback_head_disabled() {
    let app = App::builder()
        .route(("/", dummy_handler))
        .fallback_head(false)
        .finish()
        .unwrap();

    assert_matches!(
        app.router().recognize("/", &Method::HEAD),
        Err(RecognizeErrorKind::MethodNotAllowed)
    );
}

#[test]
fn route_fallback_options_enabled() {
    let app = App::builder()
        .route(("/", dummy_handler))
        .route(("/", Method::POST, dummy_handler))
        .route(("/options", Method::OPTIONS, dummy_handler))
        .fallback_options(true)
        .finish()
        .unwrap();

    assert_matches!(
        app.router().recognize("/", &Method::OPTIONS),
        Err(RecognizeErrorKind::FallbackOptions { .. })
    );
    assert_matches!(
        app.router().recognize("/options", &Method::OPTIONS),
        Ok(Recognize { endpoint_id: 2, .. })
    );
}

#[test]
fn route_fallback_options_disabled() {
    let app = App::builder()
        .route(("/", dummy_handler))
        .route(("/", Method::POST, dummy_handler))
        .fallback_options(false)
        .finish()
        .unwrap();

    assert_matches!(
        app.router().recognize("/", &Method::OPTIONS),
        Err(RecognizeErrorKind::MethodNotAllowed)
    );
}

// TODO: more test cases
#[test]
fn mount_smoke() {
    let app = App::builder()
        .mount("/", |m| {
            m.route(("/foo", dummy_handler)); // /foo
            m.route(("/bar", dummy_handler)); // /bar
        })
        .mount("/baz", |m| {
            m.route(("/", dummy_handler)); // /baz

            m.mount("/", |m| {
                m.route(("/foobar", dummy_handler)); // /baz/foobar
            });
        })
        .route(("/hoge", dummy_handler)) // /hoge
        .finish()
        .unwrap();

    assert_matches!(
        app.router().recognize("/foo", &Method::GET),
        Ok(Recognize { endpoint_id: 0, .. })
    );
    assert_matches!(
        app.router().recognize("/bar", &Method::GET),
        Ok(Recognize { endpoint_id: 1, .. })
    );
    assert_matches!(
        app.router().recognize("/baz", &Method::GET),
        Ok(Recognize { endpoint_id: 2, .. })
    );
    assert_matches!(
        app.router().recognize("/baz/foobar", &Method::GET),
        Ok(Recognize { endpoint_id: 3, .. })
    );
    assert_matches!(
        app.router().recognize("/hoge", &Method::GET),
        Ok(Recognize { endpoint_id: 4, .. })
    );

    assert_matches!(
        app.router().recognize("/baz/", &Method::GET),
        Err(RecognizeErrorKind::NotFound)
    );
}

#[test]
fn scope_variable() {
    let app = App::builder()
        .set::<String>("G".into())
        .mount("/s0", |m| {
            m.mount("/s1", |m| {
                m.set::<String>("A".into());
            });
        })
        .mount("/s2", |m| {
            m.set::<String>("B".into());
            m.mount("/s3", |m| {
                m.set::<String>("C".into());
                m.mount("/s4", |_m| {});
            }).mount("/s5", |m| {
                m.mount("/s6", |_m| {});
            });
        })
        .finish()
        .unwrap();

    assert_eq!(app.get(0).map(String::as_str), Some("G"));
    assert_eq!(app.get(1).map(String::as_str), Some("A"));
    assert_eq!(app.get(2).map(String::as_str), Some("B"));
    assert_eq!(app.get(3).map(String::as_str), Some("C"));
    assert_eq!(app.get(4).map(String::as_str), Some("C"));
    assert_eq!(app.get(5).map(String::as_str), Some("B"));
    assert_eq!(app.get(6).map(String::as_str), Some("B"));
}
