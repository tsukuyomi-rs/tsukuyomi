use {
    super::{
        imp::{Recognize, RecognizeError},
        route, scope, RouteId, ScopeId,
    },
    http::Method,
    matches::assert_matches,
};

#[test]
fn empty() {
    let app = crate::app::app().build().unwrap();
    assert_matches!(
        app.recognize("/", &Method::GET),
        Err(RecognizeError::NotFound)
    );
}

#[test]
fn route_single_method() {
    let app = crate::app::app() //
        .route(route().reply(|| ""))
        .build()
        .unwrap();

    assert_matches!(
        app.recognize("/", &Method::GET),
        Ok(Recognize::Matched(0, ..))
    );

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
    let app = crate::app::app()
        .route(route().reply(|| ""))
        .route(route().method(Method::POST).reply(|| ""))
        .build()
        .unwrap();

    assert_matches!(
        app.recognize("/", &Method::GET),
        Ok(Recognize::Matched(0, ..))
    );
    assert_matches!(
        app.recognize("/", &Method::POST),
        Ok(Recognize::Matched(1, ..))
    );

    assert_matches!(
        app.recognize("/", &Method::PUT),
        Err(RecognizeError::MethodNotAllowed)
    );
}

#[test]
fn route_fallback_head_enabled() {
    let app = crate::app::app() //
        .route(route().reply(|| ""))
        .build()
        .unwrap();

    assert_matches!(
        app.recognize("/", &Method::HEAD),
        Ok(Recognize::FallbackHead(0, ..))
    );
}

#[test]
fn route_fallback_head_disabled() {
    let app = crate::app::app() //
        .route(route().reply(|| ""))
        .fallback_head(false)
        .build()
        .unwrap();

    assert_matches!(
        app.recognize("/", &Method::HEAD),
        Err(RecognizeError::MethodNotAllowed)
    );
}

#[test]
fn route_fallback_options_enabled() {
    let app = crate::app::app() //
        .route(route().reply(|| "")) // 0
        .route(route().method(Method::POST).reply(|| "")) // 1
        .route(
            route()
                .uri("/options".parse().unwrap())
                .method(Method::OPTIONS)
                .reply(|| ""),
        ) // 2
        .build()
        .unwrap();

    assert_matches!(app.recognize("/", &Method::OPTIONS), Ok(Recognize::FallbackOptions(h)) if h == "GET, POST, OPTIONS");
    assert_matches!(
        app.recognize("/options", &Method::OPTIONS),
        Ok(Recognize::Matched(2, ..))
    );
}

#[test]
fn route_fallback_options_disabled() {
    let app = crate::app::app() //
        .route(route().reply(|| ""))
        .route(route().method(Method::POST).reply(|| ""))
        .fallback_options(false)
        .build()
        .unwrap();

    assert_matches!(
        app.recognize("/", &Method::OPTIONS),
        Err(RecognizeError::MethodNotAllowed)
    );
}

#[test]
fn asterisk_route() {
    let app = crate::app::app()
        .route(
            route()
                .uri("*".parse().unwrap())
                .method(Method::OPTIONS)
                .reply(|| "explciit OPTIONS handler"),
        ) //
        .build()
        .unwrap();

    assert_matches!(
        app.recognize("*", &Method::OPTIONS),
        Ok(Recognize::Matched(0, ..))
    );
}

#[test]
fn asterisk_route_with_normal_routes() {
    let app = crate::app::app()
        .route(route().uri("/".parse().unwrap()).reply(|| ""))
        .mount(
            scope()
                .prefix("/api".parse().unwrap())
                .route(route().uri("/posts".parse().unwrap()).reply(|| "")) //
                .route(route().uri("/events".parse().unwrap()).reply(|| "")),
        ) //
        .route(
            route()
                .uri("*".parse().unwrap())
                .method(Method::OPTIONS)
                .reply(|| "explciit OPTIONS handler"),
        ) //
        .build()
        .unwrap();

    assert_matches!(
        app.recognize("*", &Method::OPTIONS),
        Ok(Recognize::Matched(3, ..))
    );
}

#[test]
fn scope_simple() {
    let app = crate::app::app() //
        .mount(
            scope()
                .route(route().uri("/a".parse().unwrap()).reply(|| ""))
                .route(route().uri("/b".parse().unwrap()).reply(|| "")),
        ) //
        .route(route().uri("/foo".parse().unwrap()).reply(|| ""))
        .mount(
            scope()
                .prefix("/c".parse().unwrap())
                .route(route().uri("/d".parse().unwrap()).reply(|| ""))
                .route(route().uri("/e".parse().unwrap()).reply(|| "")),
        ) //
        .build()
        .unwrap();

    assert_matches!(
        app.recognize("/a", &Method::GET),
        Ok(Recognize::Matched(0, ..))
    );
    assert_matches!(
        app.recognize("/b", &Method::GET),
        Ok(Recognize::Matched(1, ..))
    );
    assert_matches!(
        app.recognize("/foo", &Method::GET),
        Ok(Recognize::Matched(2, ..))
    );
    assert_matches!(
        app.recognize("/c/d", &Method::GET),
        Ok(Recognize::Matched(3, ..))
    );
    assert_matches!(
        app.recognize("/c/e", &Method::GET),
        Ok(Recognize::Matched(4, ..))
    );
}

#[test]
fn scope_nested() {
    let app = crate::app::app()
        .mount(
            scope()
                .route(route().uri("/foo".parse().unwrap()).reply(|| "")) // /foo
                .route(route().uri("/bar".parse().unwrap()).reply(|| "")), // /bar
        ) //
        .mount(
            scope()
                .prefix("/baz".parse().unwrap())
                .route(route().reply(|| "")) // /baz
                .mount(
                    scope().route(
                        route()
                            .uri("/foobar".parse().unwrap()) // /baz/foobar
                            .reply(|| ""),
                    ),
                ), //
        ) //
        .route(route().uri("/hoge".parse().unwrap()).reply(|| "")) // /hoge
        .build()
        .unwrap();

    assert_matches!(
        app.recognize("/foo", &Method::GET),
        Ok(Recognize::Matched(0, ..))
    );
    assert_matches!(
        app.recognize("/bar", &Method::GET),
        Ok(Recognize::Matched(1, ..))
    );
    assert_matches!(
        app.recognize("/baz", &Method::GET),
        Ok(Recognize::Matched(2, ..))
    );
    assert_matches!(
        app.recognize("/baz/foobar", &Method::GET),
        Ok(Recognize::Matched(3, ..))
    );
    assert_matches!(
        app.recognize("/hoge", &Method::GET),
        Ok(Recognize::Matched(4, ..))
    );

    assert_matches!(
        app.recognize("/baz/", &Method::GET),
        Err(RecognizeError::NotFound)
    );
}

#[test]
fn scope_variable() {
    let app = crate::app::app()
        .state::<String>("G".into())
        .route(route().uri("/rg".parse().unwrap()).reply(|| ""))
        .mount(
            scope()
                .prefix("/s0".parse().unwrap())
                .route(route().uri("/r0".parse().unwrap()).reply(|| ""))
                .mount(
                    scope()
                        .prefix("/s1".parse().unwrap())
                        .state::<String>("A".into())
                        .route(route().uri("/r1".parse().unwrap()).reply(|| "")),
                ),
        ) //
        .mount(
            scope()
                .prefix("/s2".parse().unwrap())
                .state::<String>("B".into())
                .route(route().uri("/r2".parse().unwrap()).reply(|| ""))
                .mount(
                    scope()
                        .prefix("/s3".parse().unwrap())
                        .state::<String>("C".into())
                        .route(route().uri("/r3".parse().unwrap()).reply(|| ""))
                        .mount(
                            scope()
                                .prefix("/s4".parse().unwrap())
                                .route(route().uri("/r4".parse().unwrap()).reply(|| "")),
                        ),
                ) //
                .mount(
                    scope()
                        .prefix("/s5".parse().unwrap())
                        .route(route().uri("/r5".parse().unwrap()).reply(|| ""))
                        .mount(
                            scope()
                                .prefix("/s6".parse().unwrap())
                                .route(route().uri("/r6".parse().unwrap()).reply(|| "")),
                        ),
                ), //
        ) //
        .build()
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
    let app = crate::app::app()
        .route(route().uri("/path".parse().unwrap()).reply(|| ""))
        .route(route().uri("/path".parse().unwrap()).reply(|| ""))
        .build();
    assert!(app.is_err());
}

#[test]
fn failcase_different_scope_at_the_same_uri() {
    let app = crate::app::app()
        .route(route().uri("/path".parse().unwrap()).reply(|| ""))
        .mount(scope().route(route().uri("/path".parse().unwrap()).reply(|| ""))) //
        .build();
    assert!(app.is_err());
}

#[test]
fn failcase_asterisk_with_prefix() {
    let app = crate::app::app()
        .prefix("/api/v1".parse().unwrap())
        .route(route().uri("*".parse().unwrap()).reply(|| ""))
        .build();
    assert!(app.is_err());
}

#[test]
fn failcase_asterisk_without_explicit_options() {
    let app = crate::app::app()
        .route(route().uri("*".parse().unwrap()).reply(|| ""))
        .build();
    assert!(app.is_err());
}

#[test]
fn failcase_asterisk_with_explicit_get_handler() {
    let app = crate::app::app()
        .route(
            route() //
                .uri("*".parse().unwrap())
                .methods(vec![Method::GET, Method::OPTIONS])
                .reply(|| ""),
        ).build();
    assert!(app.is_err());
}
