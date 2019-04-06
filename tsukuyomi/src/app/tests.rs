use {
    super::{App, Builder, Result},
    crate::endpoint,
    crate::path,
    matches::assert_matches,
};

#[test]
fn new_empty() -> Result<()> {
    let app: App = App::builder().build()?;
    assert_matches!(app.inner.find_resource("/", &mut None), Err(..));
    Ok(())
}

#[test]
fn route_single_method() -> Result<()> {
    let app: App = App::builder()
        .root(|mut scope| {
            scope.at("/")?.to(endpoint::call(|| ""))?;
            Ok(())
        })?
        .build()?;

    assert_matches!(
        app.inner.find_resource("/", &mut None),
        Ok(endpoint) if endpoint.uri == "/"
    );

    assert_matches!(app.inner.find_resource("/path/to", &mut None), Err(..));

    assert_matches!(
        app.inner.find_resource("/", &mut None),
        Ok(endpoint) if endpoint.uri == "/"
    );

    Ok(())
}

#[test]
fn scope_simple() -> Result<()> {
    let app: App = App::builder()
        .root(|mut scope| {
            scope.mount("/")?.done(|mut scope| {
                scope.at("/a")?.to(endpoint::call(|| "a"))?;
                scope.at("/b")?.to(endpoint::call(|| "b"))
            })?;

            scope.at("/foo")?.to(endpoint::call(|| "foo"))?;

            scope.mount("/c")?.at("/d")?.to(endpoint::call(|| "c-d")) //
        })?
        .build()?;

    assert_matches!(
        app.inner.find_resource("/a", &mut None),
        Ok(endpoint) if endpoint.uri == "/a"
    );
    assert_matches!(
        app.inner.find_resource("/b", &mut None),
        Ok(endpoint) if endpoint.uri == "/b"
    );
    assert_matches!(
        app.inner.find_resource("/foo", &mut None),
        Ok(endpoint) if endpoint.uri == "/foo"
    );
    assert_matches!(
        app.inner.find_resource("/c/d", &mut None),
        Ok(endpoint) if endpoint.uri == "/c/d"
    );

    Ok(())
}

#[test]
fn scope_nested() -> Result<()> {
    let app: App = App::builder()
        .root(|mut s| {
            // 0
            s.mount("/")?.done(|mut s| {
                s.at("/foo")?.to(endpoint::call(|| ""))?; // /foo
                s.at("/bar")?.to(endpoint::call(|| "")) // /bar
            })?;

            // 1
            s.mount("/baz")?.done(|mut s| {
                s.at("/")?.to(endpoint::call(|| ""))?; // /baz

                // 2
                s.mount("/")?
                    .at("/foobar")? //
                    .to(endpoint::call(|| "")) // /baz/foobar
            })?;

            s.at("/hoge")?.to(endpoint::call(|| "")) // /hoge
        })?
        .build()?;

    assert_matches!(
        app.inner.find_resource("/foo", &mut None),
        Ok(endpoint) if endpoint.uri == "/foo"
    );
    assert_matches!(
        app.inner.find_resource("/bar", &mut None),
        Ok(endpoint) if endpoint.uri == "/bar"
    );
    assert_matches!(
        app.inner.find_resource("/foo", &mut None),
        Ok(endpoint) if endpoint.uri == "/foo"
    );
    assert_matches!(
        app.inner.find_resource("/baz", &mut None),
        Ok(endpoint) if endpoint.uri == "/baz"
    );
    assert_matches!(
        app.inner.find_resource("/baz/foobar", &mut None),
        Ok(endpoint) if endpoint.uri == "/baz/foobar"
    );
    assert_matches!(
        app.inner.find_resource("/hoge", &mut None),
        Ok(endpoint) if endpoint.uri == "/hoge"
    );

    assert_matches!(app.inner.find_resource("/baz/", &mut None), Err(..));

    Ok(())
}

#[test]
fn failcase_duplicate_uri() -> Result<()> {
    let res: Result<Builder> = App::builder() //
        .root(|mut s| {
            s.at("/path")?.get().to(endpoint::call(|| "a"))?; //
            s.at("/path")?.post().to(endpoint::call(|| "b"))
        });
    assert!(res.is_err());
    Ok(())
}

#[test]
fn failcase_different_scope_at_the_same_uri() -> Result<()> {
    let res: Result<Builder> = App::builder() //
        .root(|mut s| {
            s.at("/path")?.to(endpoint::call(|| "a"))?; //
            s.mount("/")?.at("/path")?.post().to(endpoint::call(|| "b")) //
        });
    assert!(res.is_err());
    Ok(())
}

#[test]
fn current_thread() -> Result<()> {
    use super::concurrency::current_thread::CurrentThread;
    let ptr = std::rc::Rc::new(());

    let _app: App<CurrentThread> = App::builder()
        .root(|mut s| {
            s.at("/")?.to(endpoint::call(move || {
                let _ptr = ptr.clone();
                "dummy"
            }))
        })?
        .build()?;

    Ok(())
}

#[test]
fn experimental_api() -> Result<()> {
    let _app: App = App::builder()
        .root(|mut scope| {
            scope.at("/")?.to(endpoint::call(|| "hello"))?; //

            scope.mount("/foo")?.done(|mut scope| {
                scope
                    .at(path!("/:id"))?
                    .to(endpoint::call(|_id: u32| "got id")) //
            })?;

            scope.fallback(endpoint::call(|| "fallback")) //
        })?
        .build()?;

    Ok(())
}
