use {
    super::{
        route,
        scope::{mount, state},
        Recognize, ScopeId,
    },
    http::Method,
    matches::assert_matches,
};

#[test]
fn empty() {
    let app = crate::app::app().build().unwrap();
    assert_matches!(
        app.data.recognize("/", &Method::GET),
        Recognize::NotFound(ScopeId::Global)
    );
}

#[test]
fn route_single_method() {
    let app = crate::app::app() //
        .with(route().reply(|| ""))
        .build()
        .unwrap();
    eprintln!("[dbg] app = {:#?}", app);

    assert_matches!(
        app.data.recognize("/", &Method::GET),
        Recognize::Matched { route, .. } if route.id.1 == 0
    );

    assert_matches!(
        app.data.recognize("/path/to", &Method::GET),
        Recognize::NotFound(ScopeId::Global)
    );
    assert_matches!(
        app.data.recognize("/", &Method::POST),
        Recognize::MethodNotAllowed { .. }
    );
}

#[test]
fn route_multiple_method() {
    let app = crate::app::app()
        .with(route().reply(|| ""))
        .with(route().method(Method::POST).reply(|| ""))
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
        .with(route().reply(|| ""))
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
        .with(route().reply(|| ""))
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
        .with(
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
        .with(route().uri("/".parse().unwrap()).reply(|| ""))
        .with(
            mount("/api".parse().unwrap())
                .with(route().uri("/posts".parse().unwrap()).reply(|| ""))
                .with(route().uri("/events".parse().unwrap()).reply(|| "")),
        ) //
        .with(
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
        .with(
            mount("/".parse().unwrap())
                .with(route().uri("/a".parse().unwrap()).reply(|| ""))
                .with(route().uri("/b".parse().unwrap()).reply(|| "")),
        ) //
        .with(route().uri("/foo".parse().unwrap()).reply(|| ""))
        .with(
            mount("/".parse().unwrap())
                .prefix("/c".parse().unwrap())
                .with(route().uri("/d".parse().unwrap()).reply(|| ""))
                .with(route().uri("/e".parse().unwrap()).reply(|| "")),
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
        .with(
            mount("/".parse().unwrap()) // 0
                .with(route().uri("/foo".parse().unwrap()).reply(|| "")) // /foo
                .with(route().uri("/bar".parse().unwrap()).reply(|| "")), // /bar
        ) //
        .with(
            mount("/baz".parse().unwrap()) // 1
                .with(route().reply(|| "")) // /baz
                .with(
                    mount("/".parse().unwrap()) // 2
                        .with(
                            route()
                                .uri("/foobar".parse().unwrap()) // /baz/foobar
                                .reply(|| ""),
                        ),
                ), //
        ) //
        .with(route().uri("/hoge".parse().unwrap()).reply(|| "")) // /hoge
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
        Recognize::NotFound(ScopeId::Local(2))
    );
}

#[test]
fn scope_variable() {
    let app = crate::app::app()
        .with(state::<String>("G".into()))
        .with(route().uri("/rg".parse().unwrap()).reply(|| ""))
        .with(
            mount("/s0".parse().unwrap())
                .with(route().uri("/r0".parse().unwrap()).reply(|| ""))
                .with(
                    (mount("/s1".parse().unwrap()))
                        .with(state::<String>("A".into()))
                        .with(route().uri("/r1".parse().unwrap()).reply(|| "")),
                ),
        ) //
        .with(
            mount("/s2".parse().unwrap())
                .with(state::<String>("B".into()))
                .with(route().uri("/r2".parse().unwrap()).reply(|| ""))
                .with(
                    mount("/s3".parse().unwrap())
                        .with(state::<String>("C".into()))
                        .with(route().uri("/r3".parse().unwrap()).reply(|| ""))
                        .with(
                            mount("/s4".parse().unwrap())
                                .with(route().uri("/r4".parse().unwrap()).reply(|| "")),
                        ),
                ) //
                .with(
                    mount("/s5".parse().unwrap())
                        .with(route().uri("/r5".parse().unwrap()).reply(|| ""))
                        .with(
                            mount("/s6".parse().unwrap())
                                .with(route().uri("/r6".parse().unwrap()).reply(|| "")),
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
fn scope_candidates() {
    let app = crate::app::app()
        .with(
            mount("/s0".parse().unwrap()) // 0
                .with(
                    mount("/s1".parse().unwrap()) // 1
                        .with(
                            mount("/s2".parse().unwrap()) // 2
                                .with(route().uri("/r0".parse().unwrap()).say(""))
                                .with(route().uri("/r1".parse().unwrap()).say("")),
                        ),
                ) //
                .with(route().uri("/r2".parse().unwrap()).say("")),
        ) //
        .with(
            mount("/".parse().unwrap()) // 3
                .with(route().uri("/r3".parse().unwrap()).say("")),
        ) //
        .build()
        .unwrap();

    assert_matches!(
        app.data.recognize("/s0", &Method::GET),
        Recognize::NotFound(ScopeId::Local(0))
    );

    assert_matches!(
        app.data.recognize("/s0/s1", &Method::GET),
        Recognize::NotFound(ScopeId::Local(1))
    );

    assert_matches!(
        app.data.recognize("/s0/s1/s2", &Method::GET),
        Recognize::NotFound(ScopeId::Local(2))
    );

    assert_matches!(
        app.data.recognize("/s0/r", &Method::GET),
        Recognize::NotFound(ScopeId::Local(0))
    );

    assert_matches!(
        app.data.recognize("/r", &Method::GET),
        Recognize::NotFound(ScopeId::Local(3))
    );

    assert_matches!(
        app.data.recognize("/noroute", &Method::GET),
        Recognize::NotFound(ScopeId::Global)
    );
}

#[test]
fn failcase_duplicate_uri_and_method() {
    let app = crate::app::app()
        .with(route().uri("/path".parse().unwrap()).reply(|| ""))
        .with(route().uri("/path".parse().unwrap()).reply(|| ""))
        .build();
    assert!(app.is_err());
}

#[test]
fn failcase_different_scope_at_the_same_uri() {
    let app = crate::app::app()
        .with(route().uri("/path".parse().unwrap()).reply(|| ""))
        .with(
            mount("/".parse().unwrap()) //
                .with(route().uri("/path".parse().unwrap()).reply(|| "")),
        ) //
        .build();
    assert!(app.is_err());
}

#[test]
fn failcase_asterisk_with_prefix() {
    let app = (crate::app::app().prefix("/api/v1".parse().unwrap()))
        .with(route().uri("*".parse().unwrap()).reply(|| ""))
        .build();
    assert!(app.is_err());
}

#[test]
fn failcase_asterisk_without_explicit_options() {
    let app = crate::app::app()
        .with(route().uri("*".parse().unwrap()).reply(|| ""))
        .build();
    assert!(app.is_err());
}

#[test]
fn failcase_asterisk_with_explicit_get_handler() {
    let app = crate::app::app()
        .with(
            route() //
                .uri("*".parse().unwrap())
                .methods(vec![Method::GET, Method::OPTIONS])
                .reply(|| ""),
        ).build();
    assert!(app.is_err());
}

#[allow(deprecated)]
#[test]
fn test_deprecated() {
    let app = crate::app::app()
        .route(route().uri("/".parse().unwrap()).say(""))
        .mount(
            crate::app::scope()
                .prefix("/s1".parse().unwrap())
                .route(route().uri("/".parse().unwrap()).say(""))
                .mount(crate::app::scope().route(route().uri("/a".parse().unwrap()).say(""))),
        ).build()
        .unwrap();

    assert_matches!(
        app.data.recognize("/", &Method::GET),
        Recognize::Matched { route, .. } if (route.id.0).0 == ScopeId::Global && route.id.1 == 0
    );
    assert_matches!(
        app.data.recognize("/s1", &Method::GET),
        Recognize::Matched { route, .. } if (route.id.0).0 == ScopeId::Local(0) && route.id.1 == 1
    );
    assert_matches!(
        app.data.recognize("/s1/a", &Method::GET),
        Recognize::Matched { route, .. } if (route.id.0).0 == ScopeId::Local(1) && route.id.1 == 2
    );
}
