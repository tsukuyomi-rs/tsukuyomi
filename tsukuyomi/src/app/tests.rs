use {
    super::{config::Result, App},
    crate::endpoint::builder as endpoint,
    crate::path,
    matches::assert_matches,
};

#[test]
fn new_empty() -> Result<()> {
    let app: App = App::build(|_s| Ok(()))?;
    assert_matches!(app.inner.find_endpoint("/", &mut None), Err(..));
    Ok(())
}

#[test]
fn route_single_method() -> Result<()> {
    let app: App = App::build(|s| {
        s.at("/", (), endpoint::reply(""))?;
        Ok(())
    })?;

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
    let app: App = App::build(|s| {
        s.nest("/", (), |s| {
            s.at("/a", (), endpoint::reply(""))?;
            s.at("/b", (), endpoint::reply(""))
        })?;
        s.at("/foo", (), endpoint::reply(""))?;
        s.nest("/c", (), |s| {
            s.at("/d", (), endpoint::reply("")) //
        })
    })?;

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
    let app: App = App::build(|s| {
        // 0
        s.nest("/", (), |s| {
            s.at("/foo", (), endpoint::reply(""))?; // /foo
            s.at("/bar", (), endpoint::reply("")) // /bar
        })?;

        // 1
        s.nest("/baz", (), |s| {
            s.at("/", (), endpoint::reply(""))?; // /baz

            // 2
            s.nest("/", (), |s| {
                s.at("/foobar", (), endpoint::reply("")) // /baz/foobar
            })
        })?;

        s.at("/hoge", (), endpoint::reply("")) // /hoge
    })?;

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
    let res: Result<App> = App::build(|s| {
        s.at("/path", (), {
            endpoint::get().call(|| "") //
        })?;
        s.at("/path", (), {
            endpoint::allow_only("POST, PUT")?.call(|| "") //
        })
    });
    assert!(res.is_err());
    Ok(())
}

#[test]
fn failcase_different_scope_at_the_same_uri() -> Result<()> {
    let res: Result<App> = App::build(|s| {
        s.at("/path", (), {
            endpoint::call(|| "") //
        })?;
        s.nest("/", (), |s| {
            s.at("/path", (), {
                endpoint::post().call(|| "") //
            })
        })
    });
    assert!(res.is_err());
    Ok(())
}

#[test]
fn current_thread() -> Result<()> {
    use super::concurrency::current_thread::CurrentThread;
    let ptr = std::rc::Rc::new(());

    let _app: App<CurrentThread> = App::build(|s| {
        s.at("/", (), {
            endpoint::call(move || {
                let _ptr = ptr.clone();
                "dummy"
            })
        })
    })?;

    Ok(())
}

#[test]
fn experimental_api() -> Result<()> {
    let _app: App = App::build(|scope| {
        scope.at("/", (), {
            endpoint::reply("hello") //
        })?;

        scope.nest("/foo", (), |scope| {
            scope.at(path!("/:id"), (), {
                endpoint::call(|_id: u32| "got id") //
            })
        })?;

        scope.default((), endpoint::reply("default")) //
    })?;

    Ok(())
}
