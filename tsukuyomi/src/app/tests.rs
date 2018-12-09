use {
    super::{config::prelude::*, App, ResourceId, Result},
    http::Method,
    matches::assert_matches,
};

#[test]
fn empty() -> Result<()> {
    let app = App::configure(())?;
    assert_matches!(app.inner.route("/", &mut None), Err(..));
    Ok(())
}

#[test]
fn route_single_method() -> Result<()> {
    let app = App::configure(
        route::root().say(""), //
    )?;

    assert_matches!(
        app.inner.route("/", &mut None),
        Ok(resource) if resource.id == ResourceId(0)
    );

    assert_matches!(app.inner.route("/path/to", &mut None), Err(..));

    assert_matches!(
        app.inner.route("/", &mut None),
        Ok(resource) if resource.id == ResourceId(0)
    );

    Ok(())
}

#[test]
fn route_multiple_method() -> Result<()> {
    let app = App::configure(chain![route::root()
        .methods(vec![Method::GET, Method::POST])?
        .say(""),])?;

    assert_matches!(
        app.inner.route("/", &mut None),
        Ok(resource) if resource.id == ResourceId(0)
    );
    assert_matches!(
        app.inner.route("/", &mut None),
        Ok(resource) if resource.id == ResourceId(0)
    );

    assert_matches!(
        app.inner.route("/", &mut None),
        Ok(resource) if resource.id == ResourceId(0)
    );

    Ok(())
}

#[test]
fn asterisk_route() -> Result<()> {
    let app = App::configure(
        route::asterisk() //
            .say("explicit OPTIONS handler"),
    )?;

    assert_matches!(
        app.inner.route("*", &mut None),
        Ok(resource) if resource.id == ResourceId(0)
    );

    Ok(())
}

#[test]
fn asterisk_route_with_normal_routes() -> Result<()> {
    let app = App::configure(chain![
        route::root().say(""),
        mount(
            "/api",
            chain![
                route::root().segment("posts")?.say(""),
                route::root().segment("events")?.say(""),
            ]
        ),
        route::asterisk().say("explicit OPTIONS handler"),
    ])?;

    assert_matches!(
        app.inner.route("*", &mut None),
        Ok(resource) if resource.id == ResourceId(3)
    );

    Ok(())
}

#[test]
fn scope_simple() -> Result<()> {
    let app = App::configure(chain![
        mount(
            "/",
            chain![
                route::root().segment("a")?.say(""),
                route::root().segment("b")?.say(""),
            ]
        ),
        route::root().segment("foo")?.say(""),
        mount("/c", chain![route::root().segment("d")?.say(""),]),
    ])?;

    assert_matches!(
        app.inner.route("/a", &mut None),
        Ok(resource) if resource.id == ResourceId(0)
    );
    assert_matches!(
        app.inner.route("/b", &mut None),
        Ok(resource) if resource.id == ResourceId(1)
    );
    assert_matches!(
        app.inner.route("/foo", &mut None),
        Ok(resource) if resource.id == ResourceId(2)
    );
    assert_matches!(
        app.inner.route("/c/d", &mut None),
        Ok(resource) if resource.id == ResourceId(3)
    );
    assert_matches!(
        app.inner.route("/c/d", &mut None),
        Ok(resource) if resource.id == ResourceId(3)
    );

    Ok(())
}

#[test]
fn scope_nested() -> Result<()> {
    let app = App::configure(chain![
        mount(
            "/",
            chain![
                // 0
                route::root().segment("foo")?.reply(|| ""), // /foo
                route::root().segment("bar")?.reply(|| ""), // /bar
            ]
        ),
        mount(
            "/baz",
            chain![
                // 1
                route::root().reply(|| ""), // /baz
                mount(
                    "/",
                    chain![
                        // 2
                        route::root().segment("foobar")?.reply(|| ""), // /baz/foobar
                    ]
                )
            ]
        ), //
        route::root().segment("hoge")?.reply(|| "") // /hoge
    ])?;

    assert_matches!(
        app.inner.route("/foo", &mut None),
        Ok(resource) if resource.id == ResourceId(0)
    );
    assert_matches!(
        app.inner.route("/bar", &mut None),
        Ok(resource) if resource.id == ResourceId(1)
    );
    assert_matches!(
        app.inner.route("/foo", &mut None),
        Ok(resource) if resource.id == ResourceId(0)
    );
    assert_matches!(
        app.inner.route("/baz", &mut None),
        Ok(resource) if resource.id == ResourceId(2)
    );
    assert_matches!(
        app.inner.route("/baz/foobar", &mut None),
        Ok(resource) if resource.id == ResourceId(3)
    );
    assert_matches!(
        app.inner.route("/hoge", &mut None),
        Ok(resource) if resource.id == ResourceId(4)
    );

    assert_matches!(app.inner.route("/baz/", &mut None), Err(..));

    Ok(())
}

#[test]
fn failcase_duplicate_uri() -> Result<()> {
    let app = App::configure(chain![
        route::root().segment("path")?.methods("GET")?.reply(|| ""),
        route::root()
            .segment("path")?
            .methods("POST, PUT")?
            .reply(|| ""),
    ]);
    assert!(app.is_err());
    Ok(())
}

#[test]
fn failcase_different_scope_at_the_same_uri() -> Result<()> {
    let app = App::configure(chain![
        route::root() //
            .segment("path")?
            .reply(|| ""),
        mount(
            "/",
            route::root() //
                .segment("path")?
                .methods("POST")?
                .reply(|| "")
        )
    ]);
    assert!(app.is_err());
    Ok(())
}

#[test]
fn failcase_asterisk_with_prefix() -> Result<()> {
    let app = App::with_prefix("/api/v1", {
        route::asterisk().reply(|| "") //
    });
    assert!(app.is_err());
    Ok(())
}
