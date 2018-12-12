use {
    super::{config::prelude::*, App, ResourceId, Result},
    matches::assert_matches,
};

#[test]
fn new_empty() -> Result<()> {
    let app = App::create(empty())?;
    assert_matches!(app.inner.route("/", &mut None), Err(..));
    Ok(())
}

#[test]
fn route_single_method() -> Result<()> {
    let app = App::create(
        route() //
            .to(endpoint::any().say("")),
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
    let app = App::create(
        route() //
            .to(endpoint::allow_only("GET, POST")?.say("")),
    )?;

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
fn scope_simple() -> Result<()> {
    let app = App::create(chain![
        mount("/").with(chain![
            route().segment("a")?.to(endpoint::any().say("")),
            route().segment("b")?.to(endpoint::any().say("")),
        ]),
        route().segment("foo")?.to(endpoint::any().say("")),
        mount("/c").with(route().segment("d")?.to(endpoint::any().say(""))),
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
    let app = App::create(chain![
        mount("/") // 0
            .with(chain![
                route().segment("foo")?.to(endpoint::any().say("")), // /foo
                route().segment("bar")?.to(endpoint::any().say("")), // /bar
            ]),
        mount("/baz") // 1
            .with(chain![
                route().to(endpoint::any().say("")), // /baz
                mount("/") // 2
                    .with(chain![
                        route().segment("foobar")?.to(endpoint::any().say("")), // /baz/foobar
                    ])
            ]), //
        (route().segment("hoge")?) //
            .to(endpoint::any().say("")) // /hoge
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
    let app = App::create(chain![
        route().segment("path")?.to(endpoint::get().reply(|| "")),
        route()
            .segment("path")?
            .to(endpoint::allow_only("POST, PUT")?.reply(|| "")),
    ]);
    assert!(app.is_err());
    Ok(())
}

#[test]
fn failcase_different_scope_at_the_same_uri() -> Result<()> {
    let app = App::create(chain![
        (route().segment("path")?) //
            .to(endpoint::any().reply(|| ""),),
        mount("/").with(
            (route().segment("path")?) //
                .to(endpoint::post().reply(|| ""))
        )
    ]);
    assert!(app.is_err());
    Ok(())
}
