use {
    super::{config::Result, App, LocalApp},
    crate::config::prelude::*,
    matches::assert_matches,
};

#[test]
fn new_empty() -> Result<()> {
    let app = App::create(())?;
    assert_matches!(app.inner.find_endpoint("/", &mut None), Err(..));
    Ok(())
}

#[test]
fn route_single_method() -> Result<()> {
    let app = App::create(
        path!("/") //
            .to(endpoint::reply("")),
    )?;

    assert_matches!(
        app.inner.find_endpoint("/", &mut None),
        Ok(endpoint) if endpoint.uri == "/"
    );

    assert_matches!(app.inner.find_endpoint("/path/to", &mut None), Err(..));

    assert_matches!(
        app.inner.find_endpoint("/", &mut None),
        Ok(endpoint) if endpoint.uri == "/"
    );

    Ok(())
}

#[test]
fn scope_simple() -> Result<()> {
    let app = App::create(chain![
        mount("/").with(chain![
            path!("/a").to(endpoint::reply("")),
            path!("/b").to(endpoint::reply("")),
        ]),
        path!("/foo").to(endpoint::reply("")),
        mount("/c").with(path!("/d").to(endpoint::reply(""))),
    ])?;

    assert_matches!(
        app.inner.find_endpoint("/a", &mut None),
        Ok(endpoint) if endpoint.uri == "/a"
    );
    assert_matches!(
        app.inner.find_endpoint("/b", &mut None),
        Ok(endpoint) if endpoint.uri == "/b"
    );
    assert_matches!(
        app.inner.find_endpoint("/foo", &mut None),
        Ok(endpoint) if endpoint.uri == "/foo"
    );
    assert_matches!(
        app.inner.find_endpoint("/c/d", &mut None),
        Ok(endpoint) if endpoint.uri == "/c/d"
    );

    Ok(())
}

#[test]
fn scope_nested() -> Result<()> {
    let app = App::create(chain![
        mount("/") // 0
            .with(chain![
                path!("/foo").to(endpoint::reply("")), // /foo
                path!("/bar").to(endpoint::reply("")), // /bar
            ]),
        mount("/baz") // 1
            .with(chain![
                path!("/").to(endpoint::reply("")), // /baz
                mount("/") // 2
                    .with(chain![
                        path!("/foobar").to(endpoint::reply("")), // /baz/foobar
                    ])
            ]), //
        path!("/hoge") //
            .to(endpoint::reply("")) // /hoge
    ])?;

    assert_matches!(
        app.inner.find_endpoint("/foo", &mut None),
        Ok(endpoint) if endpoint.uri == "/foo"
    );
    assert_matches!(
        app.inner.find_endpoint("/bar", &mut None),
        Ok(endpoint) if endpoint.uri == "/bar"
    );
    assert_matches!(
        app.inner.find_endpoint("/foo", &mut None),
        Ok(endpoint) if endpoint.uri == "/foo"
    );
    assert_matches!(
        app.inner.find_endpoint("/baz", &mut None),
        Ok(endpoint) if endpoint.uri == "/baz"
    );
    assert_matches!(
        app.inner.find_endpoint("/baz/foobar", &mut None),
        Ok(endpoint) if endpoint.uri == "/baz/foobar"
    );
    assert_matches!(
        app.inner.find_endpoint("/hoge", &mut None),
        Ok(endpoint) if endpoint.uri == "/hoge"
    );

    assert_matches!(app.inner.find_endpoint("/baz/", &mut None), Err(..));

    Ok(())
}

#[test]
fn failcase_duplicate_uri() -> Result<()> {
    let app = App::create(chain![
        path!("/path").to(endpoint::get().call(|| "")),
        path!("/path").to(endpoint::allow_only("POST, PUT")?.call(|| "")),
    ]);
    assert!(app.is_err());
    Ok(())
}

#[test]
fn failcase_different_scope_at_the_same_uri() -> Result<()> {
    let app = App::create(chain![
        path!("/path") //
            .to(endpoint::call(|| ""),),
        mount("/").with(
            path!("/path") //
                .to(endpoint::post().call(|| ""))
        )
    ]);
    assert!(app.is_err());
    Ok(())
}

#[test]
fn current_thread() -> Result<()> {
    let ptr = std::rc::Rc::new(());

    let _app = LocalApp::create(
        path!("/") //
            .to(endpoint::call(move || {
                let _ptr = ptr.clone();
                "dummy"
            })),
    )?;

    Ok(())
}
