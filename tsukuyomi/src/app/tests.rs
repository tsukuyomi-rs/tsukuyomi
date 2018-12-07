use {
    super::{mount, route, App, ResourceId, Result, RouterResult},
    http::Method,
    matches::assert_matches,
};

#[test]
fn empty() -> Result<()> {
    let app = App::builder().build()?;
    assert_matches!(app.inner.route("/", &Method::GET), RouterResult::NotFound { .. });
    Ok(())
}

#[test]
fn route_single_method() -> Result<()> {
    let app = App::builder() //
        .with(route::root().say(""))
        .build()?;

    assert_matches!(
        app.inner.route("/", &Method::GET),
        RouterResult::FoundEndpoint { resource, endpoint, .. }
            if resource.id == ResourceId(0)
            && endpoint.id == 0
    );

    assert_matches!(
        app.inner.route("/path/to", &Method::GET),
        RouterResult::NotFound { .. }
    );

    assert_matches!(
        app.inner.route("/", &Method::POST),
        RouterResult::FoundResource { resource, .. }
            if resource.id == ResourceId(0)
    );

    Ok(())
}

#[test]
fn route_multiple_method() -> Result<()> {
    let app = App::builder()
        .with(route::root().say(""))
        .with(route::root().methods(Method::POST)?.say(""))
        .build()?;

    assert_matches!(
        app.inner.route("/", &Method::GET),
        RouterResult::FoundEndpoint { resource, endpoint, .. }
            if resource.id == ResourceId(0)
            && endpoint.id == 0
    );
    assert_matches!(
        app.inner.route("/", &Method::POST),
        RouterResult::FoundEndpoint { resource, endpoint, .. }
            if resource.id == ResourceId(0)
            && endpoint.id == 1
    );

    assert_matches!(
        app.inner.route("/", &Method::PUT),
        RouterResult::FoundResource { resource, .. }
            if resource.id == ResourceId(0)
    );

    Ok(())
}

#[test]
fn route_multiple_method_at_same_endpoint() -> Result<()> {
    let app = App::builder()
        .with(route::root().methods("GET, POST")?.say(""))
        .build()?;

    assert_matches!(
        app.inner.route("/", &Method::GET),
        RouterResult::FoundEndpoint { resource, endpoint, .. }
            if resource.id == ResourceId(0)
            && endpoint.id == 0
    );
    assert_matches!(
        app.inner.route("/", &Method::POST),
        RouterResult::FoundEndpoint { resource, endpoint, .. }
            if resource.id == ResourceId(0)
            && endpoint.id == 0
    );

    assert_matches!(
        app.inner.route("/", &Method::PUT),
        RouterResult::FoundResource { resource, .. }
            if resource.id == ResourceId(0)
    );

    Ok(())
}

#[test]
fn asterisk_route() -> Result<()> {
    let app = App::builder()
        .with(route::asterisk().say("explicit OPTIONS handler")) //
        .build()?;

    assert_matches!(
        app.inner.route("*", &Method::OPTIONS),
        RouterResult::FoundEndpoint { resource, endpoint, .. }
            if resource.id == ResourceId(0)
            && endpoint.id == 0
    );

    Ok(())
}

#[test]
fn asterisk_route_with_normal_routes() -> Result<()> {
    let app = App::builder()
        .with(route::root().say(""))
        .with(
            mount("/api")?
                .with(route::root().segment("posts")?.say(""))
                .with(route::root().segment("events")?.say("")),
        ) //
        .with(route::asterisk().say("explicit OPTIONS handler")) //
        .build()?;

    assert_matches!(
        app.inner.route("*", &Method::OPTIONS),
        RouterResult::FoundEndpoint { resource, endpoint, .. }
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
                .with(route::root().segment("a")?.say(""))
                .with(route::root().segment("b")?.say("")),
        ) //
        .with(route::root().segment("foo")?.say(""))
        .with(
            mount("/c")?
                .with(route::root().segment("d")?.say(""))
                .with(route::root().segment("d")?.methods("POST")?.say("")),
        ) //
        .build()?;

    assert_matches!(
        app.inner.route("/a", &Method::GET),
        RouterResult::FoundEndpoint { resource, endpoint, .. }
            if resource.id == ResourceId(0)
            && endpoint.id == 0
    );
    assert_matches!(
        app.inner.route("/b", &Method::GET),
        RouterResult::FoundEndpoint { resource, endpoint, .. }
            if resource.id == ResourceId(1)
            && endpoint.id == 0
    );
    assert_matches!(
        app.inner.route("/foo", &Method::GET),
        RouterResult::FoundEndpoint { resource, endpoint, .. }
            if resource.id == ResourceId(2)
            && endpoint.id == 0
    );
    assert_matches!(
        app.inner.route("/c/d", &Method::GET),
        RouterResult::FoundEndpoint { resource, endpoint, .. }
            if resource.id == ResourceId(3)
            && endpoint.id == 0
    );
    assert_matches!(
        app.inner.route("/c/d", &Method::POST),
        RouterResult::FoundEndpoint { resource, endpoint, .. }
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
                .with(route::root().segment("foo")?.reply(|| "")) // /foo
                .with(route::root().segment("bar")?.reply(|| "")) // /bar
                .with(route::root().segment("foo")?.methods("POST")?.say("")), // foo (POST)
        ) //
        .with(
            mount("/baz")? // 1
                .with(route::root().reply(|| "")) // /baz
                .with(
                    mount("/")? // 2
                        .with(route::root().segment("foobar")?.reply(|| "")), // /baz/foobar
                ), //
        ) //
        .with(route::root().segment("hoge")?.reply(|| "")) // /hoge
        .build()?;

    assert_matches!(
        app.inner.route("/foo", &Method::GET),
        RouterResult::FoundEndpoint { resource, endpoint, .. }
            if resource.id == ResourceId(0)
            && endpoint.id == 0
    );
    assert_matches!(
        app.inner.route("/bar", &Method::GET),
        RouterResult::FoundEndpoint { resource, endpoint, .. }
            if resource.id == ResourceId(1)
            && endpoint.id == 0
    );
    assert_matches!(
        app.inner.route("/foo", &Method::POST),
        RouterResult::FoundEndpoint { resource, endpoint, .. }
            if resource.id == ResourceId(0)
            && endpoint.id == 1
    );
    assert_matches!(
        app.inner.route("/baz", &Method::GET),
        RouterResult::FoundEndpoint { resource, endpoint, .. }
            if resource.id == ResourceId( 2)
            && endpoint.id == 0
    );
    assert_matches!(
        app.inner.route("/baz/foobar", &Method::GET),
        RouterResult::FoundEndpoint { resource, endpoint, .. }
            if resource.id == ResourceId(3)
            && endpoint.id == 0
    );
    assert_matches!(
        app.inner.route("/hoge", &Method::GET),
        RouterResult::FoundEndpoint { resource, endpoint, .. }
            if resource.id == ResourceId(4)
            && endpoint.id == 0
    );

    assert_matches!(
        app.inner.route("/baz/", &Method::GET),
        RouterResult::NotFound { .. }
    );

    Ok(())
}

#[test]
fn failcase_duplicate_uri_and_method() -> Result<()> {
    let app = App::builder()
        .with(route::root().segment("path")?.reply(|| ""))
        .with(route::root().segment("path")?.reply(|| ""))
        .build();
    assert!(app.is_err());
    Ok(())
}

#[test]
fn failcase_different_scope_at_the_same_uri() -> Result<()> {
    let app = App::builder()
        .with(
            route::root() //
                .segment("path")?
                .reply(|| ""),
        )
        .with(
            mount("/")? //
                .with(
                    route::root() //
                        .segment("path")?
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
        .with(route::asterisk().reply(|| ""))
        .build();
    assert!(app.is_err());
    Ok(())
}
