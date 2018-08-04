use http::{Method, Response, StatusCode};

use super::*;
use error::HttpError;
use handler::Handle;
use input::Input;

fn dummy_handler(_: &mut Input) -> Handle {
    Handle::ready(Ok(Response::new(Default::default())))
}

#[test]
fn empty() {
    let app = App::builder().finish().unwrap();
    assert_matches!(
        app.recognize("/", &Method::GET),
        Err(ref e) if e.status() == StatusCode::NOT_FOUND
    );
}

#[test]
fn route_single_method() {
    let app = App::builder().route(("/", dummy_handler)).finish().unwrap();

    assert_matches!(app.recognize("/", &Method::GET), Ok((0, ..)));

    assert_matches!(
        app.recognize("/path/to", &Method::GET),
        Err(ref e) if e.status() == StatusCode::NOT_FOUND
    );
    assert_matches!(
        app.recognize("/", &Method::POST),
        Err(ref e) if e.status() == StatusCode::METHOD_NOT_ALLOWED
    );
}

#[test]
fn route_multiple_method() {
    let app = App::builder()
        .route(("/", dummy_handler))
        .route(("/", Method::POST, dummy_handler))
        .finish()
        .unwrap();

    assert_matches!(app.recognize("/", &Method::GET), Ok((0, ..)));
    assert_matches!(app.recognize("/", &Method::POST), Ok((1, ..)));

    assert_matches!(
        app.recognize("/", &Method::PUT),
        Err(ref e) if e.status() == StatusCode::METHOD_NOT_ALLOWED
    );
}

#[test]
fn route_fallback_head_enabled() {
    let app = App::builder().route(("/", dummy_handler)).finish().unwrap();

    assert_matches!(app.recognize("/", &Method::HEAD), Ok((0, ..)));
}

#[test]
fn route_fallback_head_disabled() {
    let app = App::builder()
        .route(("/", dummy_handler))
        .fallback_head(false)
        .finish()
        .unwrap();

    assert_matches!(
        app.recognize("/", &Method::HEAD),
        Err(ref e) if e.status() == StatusCode::METHOD_NOT_ALLOWED
    );
}

#[test]
fn route_fallback_options_enabled() {
    let app = App::builder()
        .route(("/", dummy_handler)) // 0
        .route(("/", Method::POST, dummy_handler)) // 1
        .route(("/options", Method::OPTIONS, dummy_handler)) // 2
        .finish()
        .unwrap();

    assert_matches!(app.recognize("/", &Method::OPTIONS), Ok((3, ..)));
    assert_matches!(app.recognize("/options", &Method::OPTIONS), Ok((2, ..)));
}

#[test]
fn route_fallback_options_disabled() {
    let app = App::builder()
        .route(("/", dummy_handler))
        .route(("/", Method::POST, dummy_handler))
        .default_options(false)
        .finish()
        .unwrap();

    assert_matches!(
        app.recognize("/", &Method::OPTIONS),
        Err(ref e) if e.status() == StatusCode::METHOD_NOT_ALLOWED
    );
}

#[test]
fn global_prefix() {
    let app = App::builder()
        .prefix("/api")
        .route(("/a", dummy_handler))
        .route(("/b", dummy_handler))
        .finish()
        .unwrap();

    assert_matches!(app.recognize("/api/a", &Method::GET), Ok((0, ..)));
    assert_matches!(app.recognize("/api/b", &Method::GET), Ok((1, ..)));
    assert_matches!(
        app.recognize("/a", &Method::GET),
        Err(ref e) if e.status() == StatusCode::NOT_FOUND
    );
}

#[test]
fn scope_simple() {
    use app::builder::Scope;

    let app = App::builder()
        .scope(|s: &mut Scope| {
            s.route(("/a", dummy_handler));
            s.route(("/b", dummy_handler));
        })
        .route(("/foo", dummy_handler))
        .scope(|s: &mut Scope| {
            s.prefix("/c");
            s.route(("/d", dummy_handler));
            s.route(("/e", dummy_handler));
        })
        .finish()
        .unwrap();

    assert_matches!(app.recognize("/a", &Method::GET), Ok((0, ..)));
    assert_matches!(app.recognize("/b", &Method::GET), Ok((1, ..)));
    assert_matches!(app.recognize("/foo", &Method::GET), Ok((2, ..)));
    assert_matches!(app.recognize("/c/d", &Method::GET), Ok((3, ..)));
    assert_matches!(app.recognize("/c/e", &Method::GET), Ok((4, ..)));
}

#[test]
fn scope_nested() {
    use app::builder::Scope;

    let app = App::builder()
        .scope(|s: &mut Scope| {
            s.route(("/foo", dummy_handler)); // /foo
            s.route(("/bar", dummy_handler)); // /bar
        })
        .mount("/baz", |s| {
            s.route(("/", dummy_handler)); // /baz

            s.scope(|s: &mut Scope| {
                s.route(("/foobar", dummy_handler)); // /baz/foobar
            });
        })
        .route(("/hoge", dummy_handler)) // /hoge
        .finish()
        .unwrap();

    assert_matches!(app.recognize("/foo", &Method::GET), Ok((0, ..)));
    assert_matches!(app.recognize("/bar", &Method::GET), Ok((1, ..)));
    assert_matches!(app.recognize("/baz", &Method::GET), Ok((2, ..)));
    assert_matches!(app.recognize("/baz/foobar", &Method::GET), Ok((3, ..)));
    assert_matches!(app.recognize("/hoge", &Method::GET), Ok((4, ..)));

    assert_matches!(
        app.recognize("/baz/", &Method::GET),
        Err(ref e) if e.status() == StatusCode::NOT_FOUND
    );
}

#[test]
fn scope_variable() {
    let app = App::builder()
        .set::<String>("G".into())
        .route(("/rg", dummy_handler))
        .mount("/s0", |m| {
            m.route(("/r0", dummy_handler));
            m.mount("/s1", |m| {
                m.set::<String>("A".into());
                m.route(("/r1", dummy_handler));
            });
        })
        .mount("/s2", |m| {
            m.set::<String>("B".into());
            m.route(("/r2", dummy_handler));
            m.mount("/s3", |m| {
                m.set::<String>("C".into());
                m.route(("/r3", dummy_handler));
                m.mount("/s4", |m| {
                    m.route(("/r4", dummy_handler));
                });
            }).mount("/s5", |m| {
                m.route(("/r5", dummy_handler));
                m.mount("/s6", |m| {
                    m.route(("/r6", dummy_handler));
                });
            });
        })
        .finish()
        .unwrap();

    println!("{:#?}", app);

    assert_eq!(
        app.get(RouteId(ScopeId::Global, 0)).map(String::as_str),
        Some("G")
    );
    assert_eq!(
        app.get(RouteId(ScopeId::Local(0), 1)).map(String::as_str),
        Some("G")
    );
    assert_eq!(
        app.get(RouteId(ScopeId::Local(1), 2)).map(String::as_str),
        Some("A")
    );
    assert_eq!(
        app.get(RouteId(ScopeId::Local(2), 3)).map(String::as_str),
        Some("B")
    );
    assert_eq!(
        app.get(RouteId(ScopeId::Local(3), 4)).map(String::as_str),
        Some("C")
    );
    assert_eq!(
        app.get(RouteId(ScopeId::Local(4), 5)).map(String::as_str),
        Some("C")
    );
    assert_eq!(
        app.get(RouteId(ScopeId::Local(5), 6)).map(String::as_str),
        Some("B")
    );
    assert_eq!(
        app.get(RouteId(ScopeId::Local(6), 7)).map(String::as_str),
        Some("B")
    );
}

#[test]
fn failcase_duplicate_uri_and_method() {
    let app = App::builder()
        .route(("/path", Method::GET, dummy_handler))
        .route(("/path", Method::GET, dummy_handler))
        .finish();
    assert!(app.is_err());
}

#[test]
fn failcase_different_scope_at_the_same_uri() {
    let app = App::builder()
        .route(("/path", Method::GET, dummy_handler))
        .mount("/", |scope| {
            scope.route(("/path", Method::GET, dummy_handler));
        })
        .finish();
    assert!(app.is_err());
}
