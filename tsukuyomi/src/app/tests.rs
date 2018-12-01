use {
    super::{
        directives::{mount, route, state},
        router::{ResourceId, Route},
        App, Result, ScopeId,
    },
    http::Method,
    matches::assert_matches,
};

#[test]
fn empty() -> Result<()> {
    let app = App::builder().build()?;
    assert_matches!(
        app.inner.router.route("/", &Method::GET),
        Route::NotFound(ScopeId::Global)
    );
    Ok(())
}

#[test]
fn route_single_method() -> Result<()> {
    let app = App::builder() //
        .with(route("/")?.say(""))
        .build()?;

    assert_matches!(
        app.inner.router.route("/", &Method::GET),
        Route::Matched { resource, endpoint, .. }
            if resource.id == ResourceId(ScopeId::Global, 0)
            && endpoint.id == 0
    );

    assert_matches!(
        app.inner.router.route("/path/to", &Method::GET),
        Route::NotFound(ScopeId::Global)
    );

    assert_matches!(
        app.inner.router.route("/", &Method::POST),
        Route::MethodNotAllowed { resource, .. }
            if resource.id == ResourceId(ScopeId::Global, 0)
    );

    Ok(())
}

#[test]
fn route_multiple_method() -> Result<()> {
    let app = App::builder()
        .with(route("/")?.say(""))
        .with(route("/")?.methods(Method::POST)?.say(""))
        .build()?;

    assert_matches!(
        app.inner.router.route("/", &Method::GET),
        Route::Matched { resource, endpoint, .. }
            if resource.id == ResourceId(ScopeId::Global, 0)
            && endpoint.id == 0
    );
    assert_matches!(
        app.inner.router.route("/", &Method::POST),
        Route::Matched { resource, endpoint, .. }
            if resource.id == ResourceId(ScopeId::Global, 0)
            && endpoint.id == 1
    );

    assert_matches!(
        app.inner.router.route("/", &Method::PUT),
        Route::MethodNotAllowed { resource, .. }
            if resource.id == ResourceId(ScopeId::Global, 0)
    );

    Ok(())
}

#[test]
fn route_multiple_method_at_same_endpoint() -> Result<()> {
    let app = App::builder()
        .with(route("/")?.methods("GET, POST")?.say(""))
        .build()?;

    assert_matches!(
        app.inner.router.route("/", &Method::GET),
        Route::Matched { resource, endpoint, .. }
            if resource.id == ResourceId(ScopeId::Global, 0)
            && endpoint.id == 0
    );
    assert_matches!(
        app.inner.router.route("/", &Method::POST),
        Route::Matched { resource, endpoint, .. }
            if resource.id == ResourceId(ScopeId::Global, 0)
            && endpoint.id == 0
    );

    assert_matches!(
        app.inner.router.route("/", &Method::PUT),
        Route::MethodNotAllowed { resource, .. }
            if resource.id == ResourceId(ScopeId::Global, 0)
    );

    Ok(())
}

#[test]
fn route_fallback_head_enabled() -> Result<()> {
    let app = App::builder() //
        .with(route("/")?.say(""))
        .build()?;

    assert_matches!(
        app.inner.router.route("/", &Method::HEAD),
        Route::Matched { resource, endpoint, fallback_head: true, .. }
            if resource.id == ResourceId(ScopeId::Global, 0)
            && endpoint.id == 0
    );

    Ok(())
}

#[test]
fn route_fallback_head_disabled() -> Result<()> {
    let app = App::builder() //
        .with(route("/")?.say(""))
        .fallback_head(false)
        .build()?;

    assert_matches!(
        app.inner.router.route("/", &Method::HEAD),
        Route::MethodNotAllowed { resource, .. }
            if resource.id == ResourceId(ScopeId::Global, 0)
    );

    Ok(())
}

#[test]
fn asterisk_route() -> Result<()> {
    let app = App::builder()
        .with(
            route("*")?
                .methods(Method::OPTIONS)?
                .say("explicit OPTIONS handler"),
        ) //
        .build()?;

    assert_matches!(
        app.inner.router.route("*", &Method::OPTIONS),
        Route::Matched { resource, endpoint, .. }
            if resource.id == ResourceId(ScopeId::Global, 0)
            && endpoint.id == 0
    );

    Ok(())
}

#[test]
fn asterisk_route_with_normal_routes() -> Result<()> {
    let app = App::builder()
        .with(route("/")?.say(""))
        .with(
            mount("/api")?
                .with(route("/posts")?.say(""))
                .with(route("/events")?.say("")),
        ) //
        .with(
            route("*")?
                .methods(Method::OPTIONS)?
                .say("explicit OPTIONS handler"),
        ) //
        .build()?;

    assert_matches!(
        app.inner.router.route("*", &Method::OPTIONS),
        Route::Matched { resource, endpoint, .. }
            if resource.id == ResourceId(ScopeId::Global, 3)
            && endpoint.id == 0
    );

    Ok(())
}

#[test]
fn scope_simple() -> Result<()> {
    let app = App::builder() //
        .with(
            mount("/")?
                .with(route("/a")?.say(""))
                .with(route("/b")?.say("")),
        ) //
        .with(route("/foo")?.say(""))
        .with(
            mount("/c")?
                .with(route("/d")?.say(""))
                .with(route("/d")?.methods("POST")?.say("")),
        ) //
        .build()?;

    assert_matches!(
        app.inner.router.route("/a", &Method::GET),
        Route::Matched { resource, endpoint, .. }
            if resource.id == ResourceId(ScopeId::Local(0), 0)
            && endpoint.id == 0
    );
    assert_matches!(
        app.inner.router.route("/b", &Method::GET),
        Route::Matched { resource, endpoint, .. }
            if resource.id == ResourceId(ScopeId::Local(0), 1)
            && endpoint.id == 0
    );
    assert_matches!(
        app.inner.router.route("/foo", &Method::GET),
        Route::Matched { resource, endpoint, .. }
            if resource.id == ResourceId(ScopeId::Global, 2)
            && endpoint.id == 0
    );
    assert_matches!(
        app.inner.router.route("/c/d", &Method::GET),
        Route::Matched { resource, endpoint, .. }
            if resource.id == ResourceId(ScopeId::Local(1), 3)
            && endpoint.id == 0
    );
    assert_matches!(
        app.inner.router.route("/c/d", &Method::POST),
        Route::Matched { resource, endpoint, .. }
            if resource.id == ResourceId(ScopeId::Local(1), 3)
            && endpoint.id == 1
    );

    Ok(())
}

#[test]
fn scope_nested() -> Result<()> {
    let app = App::builder()
        .with(
            mount("/")? // 0
                .with(route("/foo")?.reply(|| "")) // /foo
                .with(route("/bar")?.reply(|| "")) // /bar
                .with(route("/foo")?.methods("POST")?.say("")), // foo (POST)
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
        app.inner.router.route("/foo", &Method::GET),
        Route::Matched { resource, endpoint, .. }
            if resource.id == ResourceId(ScopeId::Local(0), 0)
            && endpoint.id == 0
    );
    assert_matches!(
        app.inner.router.route("/bar", &Method::GET),
        Route::Matched { resource, endpoint, .. }
            if resource.id == ResourceId(ScopeId::Local(0), 1)
            && endpoint.id == 0
    );
    assert_matches!(
        app.inner.router.route("/foo", &Method::POST),
        Route::Matched { resource, endpoint, .. }
            if resource.id == ResourceId(ScopeId::Local(0), 0)
            && endpoint.id == 1
    );
    assert_matches!(
        app.inner.router.route("/baz", &Method::GET),
        Route::Matched { resource, endpoint, .. }
            if resource.id == ResourceId(ScopeId::Local(1),  2)
            && endpoint.id == 0
    );
    assert_matches!(
        app.inner.router.route("/baz/foobar", &Method::GET),
        Route::Matched { resource, endpoint, .. }
            if resource.id == ResourceId(ScopeId::Local(2), 3)
            && endpoint.id == 0
    );
    assert_matches!(
        app.inner.router.route("/hoge", &Method::GET),
        Route::Matched { resource, endpoint, .. }
            if resource.id == ResourceId(ScopeId::Global, 4)
            && endpoint.id == 0
    );

    assert_matches!(
        app.inner.router.route("/baz/", &Method::GET),
        Route::NotFound(ScopeId::Local(2))
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
        app.inner.get_data(ScopeId::Global).map(String::as_str),
        Some("G")
    );
    assert_eq!(
        app.inner.get_data(ScopeId::Local(0)).map(String::as_str),
        Some("G")
    );
    assert_eq!(
        app.inner.get_data(ScopeId::Local(1)).map(String::as_str),
        Some("A")
    );
    assert_eq!(
        app.inner.get_data(ScopeId::Local(2)).map(String::as_str),
        Some("B")
    );
    assert_eq!(
        app.inner.get_data(ScopeId::Local(3)).map(String::as_str),
        Some("C")
    );
    assert_eq!(
        app.inner.get_data(ScopeId::Local(4)).map(String::as_str),
        Some("C")
    );
    assert_eq!(
        app.inner.get_data(ScopeId::Local(5)).map(String::as_str),
        Some("B")
    );
    assert_eq!(
        app.inner.get_data(ScopeId::Local(6)).map(String::as_str),
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
        app.inner.router.route("/s0", &Method::GET),
        Route::NotFound(ScopeId::Local(0))
    );

    assert_matches!(
        app.inner.router.route("/s0/s1", &Method::GET),
        Route::NotFound(ScopeId::Local(1))
    );

    assert_matches!(
        app.inner.router.route("/s0/s1/s2", &Method::GET),
        Route::NotFound(ScopeId::Local(2))
    );

    assert_matches!(
        app.inner.router.route("/s0/r", &Method::GET),
        Route::NotFound(ScopeId::Local(0))
    );

    assert_matches!(
        app.inner.router.route("/r", &Method::GET),
        Route::NotFound(ScopeId::Local(3))
    );

    assert_matches!(
        app.inner.router.route("/noroute", &Method::GET),
        Route::NotFound(ScopeId::Global)
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
        app.inner.router.route("/", &Method::GET),
        Route::Matched { resource, endpoint, .. }
            if resource.id == ResourceId(ScopeId::Global, 0)
            && endpoint.id == 0
    );
    assert_matches!(
        app.inner.router.route("/s1", &Method::GET),
        Route::Matched { resource, endpoint, .. }
            if resource.id == ResourceId(ScopeId::Local(0), 1)
            && endpoint.id == 0
    );
    assert_matches!(
        app.inner.router.route("/s1/a", &Method::GET),
        Route::Matched { resource, endpoint, .. }
            if resource.id == ResourceId(ScopeId::Local(1), 2)
            && endpoint.id == 0
    );

    Ok(())
}
