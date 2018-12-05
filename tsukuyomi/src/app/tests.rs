use {
    super::{
        directives::{mount, path},
        router::{ResourceId, Route},
        App, Result,
    },
    http::Method,
    matches::assert_matches,
};

#[test]
fn empty() -> Result<()> {
    let app = App::builder().build()?;
    assert_matches!(app.inner.router.route("/", &Method::GET), Route::NotFound { .. });
    Ok(())
}

#[test]
fn route_single_method() -> Result<()> {
    let app = App::builder() //
        .with(path::root().say(""))
        .build()?;

    assert_matches!(
        app.inner.router.route("/", &Method::GET),
        Route::FoundEndpoint { resource, endpoint, .. }
            if resource.id == ResourceId(0)
            && endpoint.id == 0
    );

    assert_matches!(
        app.inner.router.route("/path/to", &Method::GET),
        Route::NotFound { .. }
    );

    assert_matches!(
        app.inner.router.route("/", &Method::POST),
        Route::FoundResource { resource, .. }
            if resource.id == ResourceId(0)
    );

    Ok(())
}

#[test]
fn route_multiple_method() -> Result<()> {
    let app = App::builder()
        .with(path::root().say(""))
        .with(path::root().methods(Method::POST)?.say(""))
        .build()?;

    assert_matches!(
        app.inner.router.route("/", &Method::GET),
        Route::FoundEndpoint { resource, endpoint, .. }
            if resource.id == ResourceId(0)
            && endpoint.id == 0
    );
    assert_matches!(
        app.inner.router.route("/", &Method::POST),
        Route::FoundEndpoint { resource, endpoint, .. }
            if resource.id == ResourceId(0)
            && endpoint.id == 1
    );

    assert_matches!(
        app.inner.router.route("/", &Method::PUT),
        Route::FoundResource { resource, .. }
            if resource.id == ResourceId(0)
    );

    Ok(())
}

#[test]
fn route_multiple_method_at_same_endpoint() -> Result<()> {
    let app = App::builder()
        .with(path::root().methods("GET, POST")?.say(""))
        .build()?;

    assert_matches!(
        app.inner.router.route("/", &Method::GET),
        Route::FoundEndpoint { resource, endpoint, .. }
            if resource.id == ResourceId(0)
            && endpoint.id == 0
    );
    assert_matches!(
        app.inner.router.route("/", &Method::POST),
        Route::FoundEndpoint { resource, endpoint, .. }
            if resource.id == ResourceId(0)
            && endpoint.id == 0
    );

    assert_matches!(
        app.inner.router.route("/", &Method::PUT),
        Route::FoundResource { resource, .. }
            if resource.id == ResourceId(0)
    );

    Ok(())
}

#[test]
fn asterisk_route() -> Result<()> {
    let app = App::builder()
        .with(
            path::asterisk()
                .methods(Method::OPTIONS)?
                .say("explicit OPTIONS handler"),
        ) //
        .build()?;

    assert_matches!(
        app.inner.router.route("*", &Method::OPTIONS),
        Route::FoundEndpoint { resource, endpoint, .. }
            if resource.id == ResourceId(0)
            && endpoint.id == 0
    );

    Ok(())
}

#[test]
fn asterisk_route_with_normal_routes() -> Result<()> {
    let app = App::builder()
        .with(path::root().say(""))
        .with(
            mount("/api")?
                .with(path::builder().segment("posts").end().say(""))
                .with(path::builder().segment("events").end().say("")),
        ) //
        .with(
            path::asterisk()
                .methods(Method::OPTIONS)?
                .say("explicit OPTIONS handler"),
        ) //
        .build()?;

    assert_matches!(
        app.inner.router.route("*", &Method::OPTIONS),
        Route::FoundEndpoint { resource, endpoint, .. }
            if resource.id == ResourceId(3)
            && endpoint.id == 0
    );

    Ok(())
}

#[test]
fn scope_simple() -> Result<()> {
    let app = App::builder() //
        .with(
            mount("/")?
                .with(path::builder().segment("a").end().say(""))
                .with(path::builder().segment("b").end().say("")),
        ) //
        .with(path::builder().segment("foo").end().say(""))
        .with(
            mount("/c")?
                .with(path::builder().segment("d").end().say(""))
                .with(path::builder().segment("d").end().methods("POST")?.say("")),
        ) //
        .build()?;

    assert_matches!(
        app.inner.router.route("/a", &Method::GET),
        Route::FoundEndpoint { resource, endpoint, .. }
            if resource.id == ResourceId(0)
            && endpoint.id == 0
    );
    assert_matches!(
        app.inner.router.route("/b", &Method::GET),
        Route::FoundEndpoint { resource, endpoint, .. }
            if resource.id == ResourceId(1)
            && endpoint.id == 0
    );
    assert_matches!(
        app.inner.router.route("/foo", &Method::GET),
        Route::FoundEndpoint { resource, endpoint, .. }
            if resource.id == ResourceId(2)
            && endpoint.id == 0
    );
    assert_matches!(
        app.inner.router.route("/c/d", &Method::GET),
        Route::FoundEndpoint { resource, endpoint, .. }
            if resource.id == ResourceId(3)
            && endpoint.id == 0
    );
    assert_matches!(
        app.inner.router.route("/c/d", &Method::POST),
        Route::FoundEndpoint { resource, endpoint, .. }
            if resource.id == ResourceId(3)
            && endpoint.id == 1
    );

    Ok(())
}

#[test]
fn scope_nested() -> Result<()> {
    let app = App::builder()
        .with(
            mount("/")? // 0
                .with(path::builder().segment("foo").end().reply(|| "")) // /foo
                .with(path::builder().segment("bar").end().reply(|| "")) // /bar
                .with(path::builder().segment("foo").end().methods("POST")?.say("")), // foo (POST)
        ) //
        .with(
            mount("/baz")? // 1
                .with(path::root().reply(|| "")) // /baz
                .with(
                    mount("/")? // 2
                        .with(path::builder().segment("foobar").end().reply(|| "")), // /baz/foobar
                ), //
        ) //
        .with(path::builder().segment("hoge").end().reply(|| "")) // /hoge
        .build()?;

    assert_matches!(
        app.inner.router.route("/foo", &Method::GET),
        Route::FoundEndpoint { resource, endpoint, .. }
            if resource.id == ResourceId(0)
            && endpoint.id == 0
    );
    assert_matches!(
        app.inner.router.route("/bar", &Method::GET),
        Route::FoundEndpoint { resource, endpoint, .. }
            if resource.id == ResourceId(1)
            && endpoint.id == 0
    );
    assert_matches!(
        app.inner.router.route("/foo", &Method::POST),
        Route::FoundEndpoint { resource, endpoint, .. }
            if resource.id == ResourceId(0)
            && endpoint.id == 1
    );
    assert_matches!(
        app.inner.router.route("/baz", &Method::GET),
        Route::FoundEndpoint { resource, endpoint, .. }
            if resource.id == ResourceId( 2)
            && endpoint.id == 0
    );
    assert_matches!(
        app.inner.router.route("/baz/foobar", &Method::GET),
        Route::FoundEndpoint { resource, endpoint, .. }
            if resource.id == ResourceId(3)
            && endpoint.id == 0
    );
    assert_matches!(
        app.inner.router.route("/hoge", &Method::GET),
        Route::FoundEndpoint { resource, endpoint, .. }
            if resource.id == ResourceId(4)
            && endpoint.id == 0
    );

    assert_matches!(
        app.inner.router.route("/baz/", &Method::GET),
        Route::NotFound { .. }
    );

    Ok(())
}

#[test]
fn failcase_duplicate_uri_and_method() -> Result<()> {
    let app = App::builder()
        .with(path::builder().segment("path").end().reply(|| ""))
        .with(path::builder().segment("path").end().reply(|| ""))
        .build();
    assert!(app.is_err());
    Ok(())
}

#[test]
fn failcase_different_scope_at_the_same_uri() -> Result<()> {
    let app = App::builder()
        .with(path::builder().segment("path").end().reply(|| ""))
        .with(
            mount("/")? //
                .with(
                    path::builder()
                        .segment("path")
                        .end()
                        .methods("POST")?
                        .reply(|| ""),
                ),
        ) //
        .build();
    assert!(app.is_err());
    Ok(())
}

#[test]
fn failcase_asterisk_with_prefix() -> Result<()> {
    let app = App::with_prefix("/api/v1")?
        .with(path::asterisk().reply(|| ""))
        .build();
    assert!(app.is_err());
    Ok(())
}

#[test]
fn failcase_asterisk_without_explicit_options() -> Result<()> {
    let app = App::builder().with(path::asterisk().reply(|| "")).build();
    assert!(app.is_err());
    Ok(())
}

#[test]
fn failcase_asterisk_with_explicit_get_handler() -> Result<()> {
    let app = App::builder()
        .with(
            path::asterisk() //
                .methods(vec![Method::GET, Method::OPTIONS])?
                .reply(|| ""),
        ).build();
    assert!(app.is_err());
    Ok(())
}
