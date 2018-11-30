use {
    super::{
        directives::{mount, route, state},
        App, Recognize, Result, ScopeId,
    },
    http::Method,
    matches::assert_matches,
};

#[test]
fn empty() -> Result<()> {
    let app = App::builder().build()?;
    assert_matches!(
        app.data.recognize("/", &Method::GET),
        Recognize::NotFound(ScopeId::Global)
    );
    Ok(())
}

#[test]
fn route_single_method() -> Result<()> {
    let app = App::builder() //
        .with(route("/")?.reply(|| ""))
        .build()?;

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

    Ok(())
}

#[test]
fn route_multiple_method() -> Result<()> {
    let app = App::builder()
        .with(route("/")?.reply(|| ""))
        .with(route("/")?.methods(Method::POST)?.reply(|| ""))
        .build()?;

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

    Ok(())
}

#[test]
fn route_fallback_head_enabled() -> Result<()> {
    let app = App::builder() //
        .with(route("/")?.reply(|| ""))
        .build()?;

    assert_matches!(
        app.data.recognize("/", &Method::HEAD),
        Recognize::Matched { route, fallback_head: true, .. } if route.id.1 == 0
    );

    Ok(())
}

#[test]
fn route_fallback_head_disabled() -> Result<()> {
    let app = App::builder() //
        .with(route("/")?.reply(|| ""))
        .fallback_head(false)
        .build()?;

    assert_matches!(
        app.data.recognize("/", &Method::HEAD),
        Recognize::MethodNotAllowed { .. }
    );

    Ok(())
}

#[test]
fn asterisk_route() -> Result<()> {
    let app = App::builder()
        .with(
            route("*")?
                .methods(Method::OPTIONS)?
                .reply(|| "explicit OPTIONS handler"),
        ) //
        .build()?;

    assert_matches!(
        app.data.recognize("*", &Method::OPTIONS),
        Recognize::Matched { route, .. } if route.id.1 == 0
    );

    Ok(())
}

#[test]
fn asterisk_route_with_normal_routes() -> Result<()> {
    let app = App::builder()
        .with(route("/")?.reply(|| ""))
        .with(
            mount("/api")?
                .with(route("/posts")?.reply(|| ""))
                .with(route("/events")?.reply(|| "")),
        ) //
        .with(
            route("*")?
                .methods(Method::OPTIONS)?
                .reply(|| "explicit OPTIONS handler"),
        ) //
        .build()?;

    assert_matches!(
        app.data.recognize("*", &Method::OPTIONS),
        Recognize::Matched { route, .. } if route.id.1 == 3
    );

    Ok(())
}

#[test]
fn scope_simple() -> Result<()> {
    let app = App::builder() //
        .with(
            mount("/")?
                .with(route("/a")?.reply(|| ""))
                .with(route("/b")?.reply(|| "")),
        ) //
        .with(route("/foo")?.reply(|| ""))
        .with(
            mount("/c")?
                .with(route("/d")?.reply(|| ""))
                .with(route("/e")?.reply(|| "")),
        ) //
        .build()?;

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

    Ok(())
}

#[test]
fn scope_nested() -> Result<()> {
    let app = App::builder()
        .with(
            mount("/")? // 0
                .with(route("/foo")?.reply(|| "")) // /foo
                .with(route("/bar")?.reply(|| "")), // /bar
        ) //
        .with(
            mount("/baz")? // 1
                .with(route("/")?.reply(|| "")) // /baz
                .with(
                    mount("/")? // 2
                        .with(route("/foobar")?.reply(|| "")), // /baz/foobar
                ), //
        ) //
        .with(route("/hoge")?.reply(|| "")) // /hoge
        .build()?;

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

    Ok(())
}

#[test]
fn scope_variable() -> Result<()> {
    let app = App::builder()
        .with(state::<String>("G".into()))
        .with(route("/rg")?.reply(|| ""))
        .with(
            mount("/s0")?.with(route("/r0")?.reply(|| "")).with(
                mount("/s1")?
                    .with(state::<String>("A".into()))
                    .with(route("/r1")?.reply(|| "")),
            ),
        ) //
        .with(
            mount("/s2")?
                .with(state::<String>("B".into()))
                .with(route("/r2")?.reply(|| ""))
                .with(
                    mount("/s3")?
                        .with(state::<String>("C".into()))
                        .with(route("/r3")?.reply(|| ""))
                        .with(mount("/s4")?.with(route("/r4")?.reply(|| ""))),
                ) //
                .with(
                    mount("/s5")?
                        .with(route("/r5")?.reply(|| ""))
                        .with(mount("/s6")?.with(route("/r6")?.reply(|| ""))),
                ), //
        ) //
        .build()?;

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

    Ok(())
}

#[test]
fn scope_candidates() -> Result<()> {
    let app = App::builder()
        .with(
            mount("/s0")? // 0
                .with(
                    mount("/s1")? // 1
                        .with(
                            mount("/s2")? // 2
                                .with(route("/r0")?.say(""))
                                .with(route("/r1")?.say("")),
                        ),
                ) //
                .with(route("/r2")?.say("")),
        ) //
        .with(
            mount("/")? // 3
                .with(route("/r3")?.say("")),
        ) //
        .build()?;

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

    Ok(())
}

#[test]
fn failcase_duplicate_uri_and_method() -> Result<()> {
    let app = App::builder()
        .with(route("/path")?.reply(|| ""))
        .with(route("/path")?.reply(|| ""))
        .build();
    assert!(app.is_err());
    Ok(())
}

#[test]
fn failcase_different_scope_at_the_same_uri() -> Result<()> {
    let app = App::builder()
        .with(route("/path")?.reply(|| ""))
        .with(
            mount("/")? //
                .with(route("/path")?.reply(|| "")),
        ) //
        .build();
    assert!(app.is_err());
    Ok(())
}

#[test]
fn failcase_asterisk_with_prefix() -> Result<()> {
    let app = App::with_prefix("/api/v1")?
        .with(route("*")?.reply(|| ""))
        .build();
    assert!(app.is_err());
    Ok(())
}

#[test]
fn failcase_asterisk_without_explicit_options() -> Result<()> {
    let app = App::builder().with(route("*")?.reply(|| "")).build();
    assert!(app.is_err());
    Ok(())
}

#[test]
fn failcase_asterisk_with_explicit_get_handler() -> Result<()> {
    let app = App::builder()
        .with(
            route("*")? //
                .methods(vec![Method::GET, Method::OPTIONS])?
                .reply(|| ""),
        ).build();
    assert!(app.is_err());
    Ok(())
}

#[allow(deprecated)]
#[test]
fn test_deprecated() -> Result<()> {
    let app = crate::app::app()
        .route(crate::app::route().uri("/".parse()?).say(""))
        .mount(
            crate::app::scope()
                .prefix("/s1".parse()?)
                .route(crate::app::route().uri("/".parse()?).say(""))
                .mount(crate::app::scope().route(crate::app::route().uri("/a".parse()?).say(""))),
        ).build()?;

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

    Ok(())
}
