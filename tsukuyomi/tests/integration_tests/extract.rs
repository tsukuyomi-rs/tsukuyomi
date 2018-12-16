use {
    http::Request,
    tsukuyomi::{
        config::prelude::*, //
        extractor,
        extractor::ExtractorExt,
        App,
    },
};

#[test]
fn unit_input() -> tsukuyomi::test::Result<()> {
    let app = App::create({
        path!(/) //
            .to(endpoint::any().call(|| "dummy"))
    })?;

    let mut server = tsukuyomi::test::server(app)?;

    let response = server.perform("/")?;
    assert_eq!(response.status(), 200);
    Ok(())
}

#[test]
fn params() -> tsukuyomi::test::Result<()> {
    let app = App::create({
        path!(/ { path::param("id") } / { path::param("name") } / { path::catch_all("path") })
            .to(endpoint::any()
                .call(|id: u32, name: String, path: String| format!("{},{},{}", id, name, path)))
    })?;

    let mut server = tsukuyomi::test::server(app)?;

    let response = server.perform("/23/bob/path/to/file")?;
    assert_eq!(response.body().to_utf8()?, "23,bob,path/to/file");

    let response = server.perform("/42/alice/")?;
    assert_eq!(response.body().to_utf8()?, "42,alice,");

    Ok(())
}

#[test]
fn route_macros() -> tsukuyomi::app::Result<()> {
    App::create(chain![
        path!(/"root") //
            .to(endpoint::any().call(|| "root")),
        path!(/ "params" / {path::param("id")} / {path::param("name")}) //
            .to(endpoint::any().call(|id: i32, name: String| {
                drop((id, name));
                "dummy"
            })),
        path!(/ "posts" / {path::param("id")} / "edit") //
            .to({
                endpoint::put()
                    .extract(extractor::body::plain::<String>())
                    .call(|id: u32, body: String| {
                        drop((id, body));
                        "dummy"
                    })
            }),
        path!(/ "static" / {path::catch_all("path")}) //
            .to(endpoint::any().call(|path: String| {
                drop(path);
                "dummy"
            })),
    ])
    .map(drop)
}

#[test]
fn plain_body() -> tsukuyomi::test::Result<()> {
    let app = App::create(
        path!(/) //
            .to(endpoint::post()
                .extract(extractor::body::plain())
                .call(|body: String| body)),
    )?;
    let mut server = tsukuyomi::test::server(app)?;

    const BODY: &[u8] = b"The quick brown fox jumps over the lazy dog";

    let response = server.perform(
        Request::post("/")
            .header("content-type", "text/plain; charset=utf-8")
            .body(BODY),
    )?;
    assert_eq!(response.status(), 200);

    // missing content-type
    let response = server.perform(Request::post("/").body(BODY))?;
    assert_eq!(response.status(), 200);

    // invalid content-type
    let response = server.perform(
        Request::post("/")
            .header("content-type", "application/graphql")
            .body(BODY),
    )?;
    assert_eq!(response.status(), 400);

    // invalid charset
    let response = server.perform(
        Request::post("/")
            .header("content-type", "text/plain; charset=euc-jp")
            .body(BODY),
    )?;
    assert_eq!(response.status(), 400);

    Ok(())
}

#[test]
fn json_body() -> tsukuyomi::test::Result<()> {
    #[derive(Debug, serde::Deserialize)]
    struct Params {
        id: u32,
        name: String,
    }

    let app = App::create(
        path!(/) //
            .to(endpoint::post()
                .extract(extractor::body::json())
                .call(|params: Params| format!("{},{}", params.id, params.name))),
    )?;
    let mut server = tsukuyomi::test::server(app)?;

    let response = server.perform(
        Request::post("/")
            .header("content-type", "application/json")
            .body(&br#"{"id":23, "name":"bob"}"#[..]),
    )?;
    assert_eq!(response.body().to_utf8()?, "23,bob");

    // missing content-type
    let response = server.perform(Request::post("/").body(&br#"{"id":23, "name":"bob"}"#[..]))?;
    assert_eq!(response.status(), 400);

    // invalid content-type
    let response = server.perform(
        Request::post("/")
            .header("content-type", "application/graphql")
            .body(&br#"{"id":23, "name":"bob"}"#[..]),
    )?;
    assert_eq!(response.status(), 400);

    // invalid data
    let response = server.perform(
        Request::post("/")
            .header("content-type", "application/json")
            .body(&br#"THIS_IS_INVALID_JSON_DATA"#[..]),
    )?;
    assert_eq!(response.status(), 400);

    Ok(())
}

#[test]
fn urlencoded_body() -> tsukuyomi::test::Result<()> {
    #[derive(Debug, serde::Deserialize)]
    struct Params {
        id: u32,
        name: String,
    }

    let app = App::create(
        path!(/) //
            .to(endpoint::post()
                .extract(extractor::body::urlencoded())
                .call(|params: Params| format!("{},{}", params.id, params.name))),
    )?;
    let mut server = tsukuyomi::test::server(app)?;

    const BODY: &[u8] = b"id=23&name=bob";

    let response = server.perform(
        Request::post("/")
            .header("content-type", "application/x-www-form-urlencoded")
            .body(BODY),
    )?;
    assert_eq!(response.body().to_utf8()?, "23,bob");

    // missing content-type
    let response = server.perform(Request::post("/").body(BODY))?;
    assert_eq!(response.status(), 400);

    // invalid content-type
    let response = server.perform(
        Request::post("/")
            .header("content-type", "application/graphql")
            .body(BODY),
    )?;
    assert_eq!(response.status(), 400);

    // invalid data
    let response = server.perform(
        Request::post("/")
            .header("content-type", "application/x-www-form-urlencoded")
            .body(&br#"THIS_IS_INVALID_FORM_DATA"#[..]),
    )?;
    assert_eq!(response.status(), 400);

    Ok(())
}

#[test]
fn local_data() -> tsukuyomi::test::Result<()> {
    use {
        futures01::Poll,
        tsukuyomi::{
            handler::{AllowedMethods, Handle, Handler, ModifyHandler},
            input::{localmap::local_key, Input},
        },
    };

    #[derive(Clone)]
    struct MyData(String);

    impl MyData {
        local_key! {
            const KEY: Self;
        }
    }

    #[derive(Clone, Default)]
    struct InsertMyData(());

    impl<H: Handler> ModifyHandler<H> for InsertMyData {
        type Output = H::Output;
        type Handler = InsertMyDataHandler<H>;

        fn modify(&self, inner: H) -> Self::Handler {
            InsertMyDataHandler(inner)
        }
    }

    struct InsertMyDataHandler<H>(H);

    impl<H: Handler> Handler for InsertMyDataHandler<H> {
        type Output = H::Output;
        type Error = H::Error;
        type Handle = InsertMyDataHandle<H::Handle>;

        fn allowed_methods(&self) -> Option<&AllowedMethods> {
            self.0.allowed_methods()
        }

        fn handle(&self) -> Self::Handle {
            InsertMyDataHandle {
                handle: self.0.handle(),
            }
        }
    }

    #[allow(missing_debug_implementations)]
    struct InsertMyDataHandle<H> {
        handle: H,
    }

    impl<H: Handle> Handle for InsertMyDataHandle<H> {
        type Output = H::Output;
        type Error = H::Error;

        fn poll_ready(&mut self, input: &mut Input) -> Poll<Self::Output, Self::Error> {
            input
                .locals
                .entry(&MyData::KEY)
                .or_insert_with(|| MyData("dummy".into()));
            self.handle.poll_ready(input)
        }
    }

    let app = App::create(
        path!(/) //
            .to({
                endpoint::any()
                    .extract(extractor::local::remove(&MyData::KEY))
                    .call(|x: MyData| x.0)
            })
            .modify(InsertMyData::default()),
    )?;
    let mut server = tsukuyomi::test::server(app)?;

    let response = server.perform("/")?;
    assert_eq!(response.status(), 200);

    Ok(())
}

#[test]
fn missing_local_data() -> tsukuyomi::test::Result<()> {
    use tsukuyomi::input::localmap::local_key;

    #[derive(Clone)]
    struct MyData(String);

    impl MyData {
        local_key! {
            const KEY: Self;
        }
    }

    let app = App::create({
        path!(/) //
            .to(endpoint::any()
                .extract(extractor::local::remove(&MyData::KEY))
                .call(|x: MyData| x.0))
    })?;
    let mut server = tsukuyomi::test::server(app)?;

    let response = server.perform("/")?;
    assert_eq!(response.status(), 500);

    Ok(())
}

#[test]
fn optional() -> tsukuyomi::test::Result<()> {
    #[derive(Debug, serde::Deserialize)]
    struct Params {
        id: u32,
        name: String,
    }

    let extractor = extractor::body::json().optional();

    let app = App::create(
        path!(/) //
            .to({
                endpoint::post() //
                    .extract(extractor)
                    .call(|params: Option<Params>| {
                        if let Some(params) = params {
                            Ok(format!("{},{}", params.id, params.name))
                        } else {
                            Err(tsukuyomi::error::internal_server_error("####none####"))
                        }
                    })
            }),
    )?;
    let mut server = tsukuyomi::test::server(app)?;

    let response = server.perform(
        Request::post("/")
            .header("content-type", "application/json")
            .body(&br#"{"id":23, "name":"bob"}"#[..]),
    )?;
    assert_eq!(response.status(), 200);
    assert_eq!(response.body().to_utf8()?, "23,bob");

    let response = server.perform(
        Request::post("/")
            .header("content-type", "application/x-www-form-urlencoded")
            .body(&b"id=23&name=bob"[..]),
    )?;
    assert_eq!(response.status(), 500);
    assert_eq!(response.body().to_utf8()?, "####none####");

    Ok(())
}

#[test]
fn either_or() -> tsukuyomi::test::Result<()> {
    #[derive(Debug, serde::Deserialize)]
    struct Params {
        id: u32,
        name: String,
    }

    let params_extractor = extractor::method::get()
        .chain(extractor::query::query())
        .either_or(extractor::method::post().chain(extractor::body::json()))
        .either_or(extractor::method::post().chain(extractor::body::urlencoded()));

    let app = App::create(
        path!(/) //
            .to({
                endpoint::allow_only("GET, POST")?
                    .extract(params_extractor)
                    .call(|params: Params| format!("{},{}", params.id, params.name))
            }),
    )?;
    let mut server = tsukuyomi::test::server(app)?;

    let response = server.perform("/?id=23&name=bob")?;
    assert_eq!(response.status(), 200);
    assert_eq!(response.body().to_utf8()?, "23,bob");

    let response = server.perform(
        Request::post("/")
            .header("content-type", "application/json")
            .body(&br#"{"id":23, "name":"bob"}"#[..]),
    )?;
    assert_eq!(response.status(), 200);
    assert_eq!(response.body().to_utf8()?, "23,bob");

    let response = server.perform(
        Request::post("/")
            .header("content-type", "application/x-www-form-urlencoded")
            .body(&b"id=23&name=bob"[..]),
    )?;
    assert_eq!(response.status(), 200);
    assert_eq!(response.body().to_utf8()?, "23,bob");

    let response = server.perform(
        Request::post("/")
            .header("content-type", "text/plain; charset=utf-8")
            .body(&b"///invalid string"[..]),
    )?;
    assert_eq!(response.status(), 400);

    Ok(())
}
