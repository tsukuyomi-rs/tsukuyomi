use {
    super::{route, scope, Recognize, ScopeId},
    http::Method,
    matches::assert_matches,
};

#[test]
fn empty() {
    let app = crate::app::app().build().unwrap();
    assert_matches!(app.data.recognize("/", &Method::GET), Recognize::NotFound);
}

#[test]
fn route_single_method() {
    let app = crate::app::app() //
        .route(route().reply(|| ""))
        .build()
        .unwrap();

    assert_matches!(
        app.data.recognize("/", &Method::GET),
        Recognize::Matched { route, .. } if route.id.1 == 0
    );

    assert_matches!(
        app.data.recognize("/path/to", &Method::GET),
        Recognize::NotFound
    );
    assert_matches!(
        app.data.recognize("/", &Method::POST),
        Recognize::MethodNotAllowed { .. }
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
        app.data.recognize("/", &Method::GET),
        Recognize::Matched { route, .. } if route.id.1 == 0
    );
    assert_matches!(
        app.data.recognize("/", &Method::POST),
        Recognize::Matched { route, .. } if route.id.1 == 1
    );

    assert_matches!(
        app.data.recognize("/", &Method::PUT),
        Recognize::MethodNotAllowed { .. }
    );
}

#[test]
fn route_fallback_head_enabled() {
    let app = crate::app::app() //
        .route(route().reply(|| ""))
        .build()
        .unwrap();

    assert_matches!(
        app.data.recognize("/", &Method::HEAD),
        Recognize::Matched { route, fallback_head: true, .. } if route.id.1 == 0
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
        app.data.recognize("/", &Method::HEAD),
        Recognize::MethodNotAllowed { .. }
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
        app.data.recognize("*", &Method::OPTIONS),
        Recognize::Matched { route, .. } if route.id.1 == 0
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
        app.data.recognize("*", &Method::OPTIONS),
        Recognize::Matched { route, .. } if route.id.1 == 3
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
        app.data.recognize("/a", &Method::GET),
        Recognize::Matched { route, .. } if route.id.1 == 0
    );
    assert_matches!(
        app.data.recognize("/b", &Method::GET),
        Recognize::Matched { route, .. } if route.id.1 == 1
    );
    assert_matches!(
        app.data.recognize("/foo", &Method::GET),
        Recognize::Matched { route, .. } if route.id.1 == 2
    );
    assert_matches!(
        app.data.recognize("/c/d", &Method::GET),
        Recognize::Matched { route, .. } if route.id.1 == 3
    );
    assert_matches!(
        app.data.recognize("/c/e", &Method::GET),
        Recognize::Matched { route, .. } if route.id.1 == 4
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
        app.data.recognize("/foo", &Method::GET),
        Recognize::Matched { route, .. } if route.id.1 == 0
    );
    assert_matches!(
        app.data.recognize("/bar", &Method::GET),
        Recognize::Matched { route, .. } if route.id.1 == 1
    );
    assert_matches!(
        app.data.recognize("/baz", &Method::GET),
        Recognize::Matched { route, .. } if route.id.1 == 2
    );
    assert_matches!(
        app.data.recognize("/baz/foobar", &Method::GET),
        Recognize::Matched { route, .. } if route.id.1 == 3
    );
    assert_matches!(
        app.data.recognize("/hoge", &Method::GET),
        Recognize::Matched { route, .. } if route.id.1 == 4
    );

    assert_matches!(
        app.data.recognize("/baz/", &Method::GET),
        Recognize::NotFound
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
        app.data.get_state(ScopeId::Global).map(String::as_str),
        Some("G")
    );
    assert_eq!(
        app.data.get_state(ScopeId::Local(0)).map(String::as_str),
        Some("G")
    );
    assert_eq!(
        app.data.get_state(ScopeId::Local(1)).map(String::as_str),
        Some("A")
    );
    assert_eq!(
        app.data.get_state(ScopeId::Local(2)).map(String::as_str),
        Some("B")
    );
    assert_eq!(
        app.data.get_state(ScopeId::Local(3)).map(String::as_str),
        Some("C")
    );
    assert_eq!(
        app.data.get_state(ScopeId::Local(4)).map(String::as_str),
        Some("C")
    );
    assert_eq!(
        app.data.get_state(ScopeId::Local(5)).map(String::as_str),
        Some("B")
    );
    assert_eq!(
        app.data.get_state(ScopeId::Local(6)).map(String::as_str),
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
