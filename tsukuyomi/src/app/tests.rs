use super::*;

use http::Method;
use matches::assert_matches;

#[test]
fn empty() {
    let app = App::builder().finish().unwrap();
    assert_matches!(
        app.recognize("/", &Method::GET),
        Err(RecognizeError::NotFound)
    );
}

#[test]
fn route_single_method() {
    let app = App::builder()
        .route(Route::index().reply(|| ""))
        .finish()
        .unwrap();

    assert_matches!(app.recognize("/", &Method::GET), Ok((0, ..)));

    assert_matches!(
        app.recognize("/path/to", &Method::GET),
        Err(RecognizeError::NotFound)
    );
    assert_matches!(
        app.recognize("/", &Method::POST),
        Err(RecognizeError::MethodNotAllowed)
    );
}

#[test]
fn route_multiple_method() {
    let app = App::builder()
        .route(Route::get("/").reply(|| ""))
        .route(Route::post("/").reply(|| ""))
        .finish()
        .unwrap();

    assert_matches!(app.recognize("/", &Method::GET), Ok((0, ..)));
    assert_matches!(app.recognize("/", &Method::POST), Ok((1, ..)));

    assert_matches!(
        app.recognize("/", &Method::PUT),
        Err(RecognizeError::MethodNotAllowed)
    );
}

#[test]
fn route_fallback_head_enabled() {
    let app = App::builder()
        .route(Route::index().reply(|| ""))
        .finish()
        .unwrap();

    assert_matches!(app.recognize("/", &Method::HEAD), Ok((0, ..)));
}

#[test]
fn route_fallback_head_disabled() {
    let app = App::builder()
        .route(Route::index().reply(|| ""))
        .config(|cfg| {
            cfg.fallback_head(false);
        }) //
        .finish()
        .unwrap();

    assert_matches!(
        app.recognize("/", &Method::HEAD),
        Err(RecognizeError::MethodNotAllowed)
    );
}

#[test]
fn route_fallback_options_enabled() {
    let app = App::builder()
        .route(Route::get("/").reply(|| "")) // 0
        .route(Route::post("/").reply(|| "")) // 1
        .route(Route::options("/options").reply(|| "")) // 2
        .finish()
        .unwrap();

    assert_matches!(app.recognize("/", &Method::OPTIONS), Ok((3, ..)));
    assert_matches!(app.recognize("/options", &Method::OPTIONS), Ok((2, ..)));
}

#[test]
fn route_fallback_options_disabled() {
    let app = App::builder()
        .route(Route::index().reply(|| ""))
        .route(Route::post("/").reply(|| ""))
        .config(|cfg| {
            cfg.fallback_options(false);
        }) //
        .finish()
        .unwrap();

    assert_matches!(
        app.recognize("/", &Method::OPTIONS),
        Err(RecognizeError::MethodNotAllowed)
    );
}

#[test]
fn scope_simple() {
    let app = App::builder()
        .mount("/", |s| {
            s.route(Route::get("/a").reply(|| ""));
            s.route(Route::get("/b").reply(|| ""));
            s.done()
        }) //
        .unwrap()
        .route(Route::get("/foo").reply(|| ""))
        .mount("/c", |scope| {
            scope
                .route(Route::get("/d").reply(|| ""))
                .route(Route::get("/e").reply(|| ""))
                .done()
        }) //
        .unwrap()
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
    let app = App::builder()
        .mount("/", |scope| {
            scope
                .route(Route::get("/foo").reply(|| "")) // /foo
                .route(Route::get("/bar").reply(|| "")) // /bar
                .done()
        }) //
        .unwrap()
        .mount("/baz", |scope| {
            scope
                .route(Route::index().reply(|| "")) // /baz
                .mount("/", |scope| {
                    scope
                        .route(Route::get("/foobar").reply(|| "")) // /baz/foobar
                        .done()
                })? //
                .done()
        }) //
        .unwrap()
        .route(Route::get("/hoge").reply(|| "")) // /hoge
        .finish()
        .unwrap();

    assert_matches!(app.recognize("/foo", &Method::GET), Ok((0, ..)));
    assert_matches!(app.recognize("/bar", &Method::GET), Ok((1, ..)));
    assert_matches!(app.recognize("/baz", &Method::GET), Ok((2, ..)));
    assert_matches!(app.recognize("/baz/foobar", &Method::GET), Ok((3, ..)));
    assert_matches!(app.recognize("/hoge", &Method::GET), Ok((4, ..)));

    assert_matches!(
        app.recognize("/baz/", &Method::GET),
        Err(RecognizeError::NotFound)
    );
}

#[test]
fn scope_variable() {
    let app = App::builder()
        .state::<String>("G".into())
        .route(Route::get("/rg").reply(|| ""))
        .mount("/s0", |scope| {
            scope
                .route(Route::get("/r0").reply(|| ""))
                .mount("/s1", |scope| {
                    scope
                        .state::<String>("A".into())
                        .route(Route::get("/r1").reply(|| ""))
                        .done()
                })? //
                .done()
        }) //
        .unwrap()
        .mount("/s2", |scope| {
            scope
                .state::<String>("B".into())
                .route(Route::get("/r2").reply(|| ""))
                .mount("/s3", |scope| {
                    scope
                        .state::<String>("C".into())
                        .route(Route::get("/r3").reply(|| ""))
                        .mount("/s4", |scope| {
                            scope.route(Route::get("/r4").reply(|| "")).done()
                        })? //
                        .done()
                })? //
                .mount("/s5", |scope| {
                    scope
                        .route(Route::get("/r5").reply(|| ""))
                        .mount("/s6", |scope| {
                            scope.route(Route::get("/r6").reply(|| "")).done()
                        })? //
                        .done()
                })? //
                .done()
        }) //
        .unwrap()
        .finish()
        .unwrap();

    assert_eq!(
        app.get_state(RouteId(ScopeId::Global, 0))
            .map(String::as_str),
        Some("G")
    );
    assert_eq!(
        app.get_state(RouteId(ScopeId::Local(0), 1))
            .map(String::as_str),
        Some("G")
    );
    assert_eq!(
        app.get_state(RouteId(ScopeId::Local(1), 2))
            .map(String::as_str),
        Some("A")
    );
    assert_eq!(
        app.get_state(RouteId(ScopeId::Local(2), 3))
            .map(String::as_str),
        Some("B")
    );
    assert_eq!(
        app.get_state(RouteId(ScopeId::Local(3), 4))
            .map(String::as_str),
        Some("C")
    );
    assert_eq!(
        app.get_state(RouteId(ScopeId::Local(4), 5))
            .map(String::as_str),
        Some("C")
    );
    assert_eq!(
        app.get_state(RouteId(ScopeId::Local(5), 6))
            .map(String::as_str),
        Some("B")
    );
    assert_eq!(
        app.get_state(RouteId(ScopeId::Local(6), 7))
            .map(String::as_str),
        Some("B")
    );
}

#[test]
fn failcase_duplicate_uri_and_method() {
    let app = App::builder()
        .route(Route::get("/path").reply(|| ""))
        .route(Route::get("/path").reply(|| ""))
        .finish();
    assert!(app.is_err());
}

#[test]
fn failcase_different_scope_at_the_same_uri() {
    let app = App::builder()
        .route(Route::get("/path").reply(|| ""))
        .mount("/", |scope| {
            scope.route(Route::get("/path").reply(|| "")).done()
        }) //
        .unwrap()
        .finish();
    assert!(app.is_err());
}
