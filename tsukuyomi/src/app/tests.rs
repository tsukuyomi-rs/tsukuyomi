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
        path!(/) //
            .to(endpoint::any().reply("")),
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
        path!(/) //
            .to(endpoint::allow_only("GET, POST")?.reply("")),
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
            path!(/"a").to(endpoint::any().reply("")),
            path!(/"b").to(endpoint::any().reply("")),
        ]),
        path!(/"foo").to(endpoint::any().reply("")),
        mount("/c").with(path!(/"d").to(endpoint::any().reply(""))),
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
                path!(/"foo").to(endpoint::any().reply("")), // /foo
                path!(/"bar").to(endpoint::any().reply("")), // /bar
            ]),
        mount("/baz") // 1
            .with(chain![
                path!(/).to(endpoint::any().reply("")), // /baz
                mount("/") // 2
                    .with(chain![
                        path!(/"foobar").to(endpoint::any().reply("")), // /baz/foobar
                    ])
            ]), //
        path!(/"hoge") //
            .to(endpoint::any().reply("")) // /hoge
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
        path!(/"path").to(endpoint::get().call(|| "")),
        path!(/"path").to(endpoint::allow_only("POST, PUT")?.call(|| "")),
    ]);
    assert!(app.is_err());
    Ok(())
}

#[test]
fn failcase_different_scope_at_the_same_uri() -> Result<()> {
    let app = App::create(chain![
        path!(/"path") //
            .to(endpoint::any().call(|| ""),),
        mount("/").with(
            path!(/"path") //
                .to(endpoint::post().call(|| ""))
        )
    ]);
    assert!(app.is_err());
    Ok(())
}
