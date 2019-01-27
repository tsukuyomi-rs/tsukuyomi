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
fn unit_input() -> izanami::Result<()> {
    let app = App::create({
        path!("/") //
            .to(endpoint::call(|| "dummy"))
    })?;

    let mut server = izanami::test::server(app)?;

    let response = server.perform("/")?;
    assert_eq!(response.status(), 200);
    Ok(())
}

#[test]
fn params() -> izanami::Result<()> {
    let app = App::create(path!("/:id/:name/*path").to(endpoint::call(
        |id: u32, name: String, path: String| format!("{},{},{}", id, name, path),
    )))?;

    let mut server = izanami::test::server(app)?;

    let response = server.perform("/23/bob/path/to/file")?;
    assert_eq!(response.body().to_utf8()?, "23,bob,path/to/file");

    let response = server.perform("/42/alice/")?;
    assert_eq!(response.body().to_utf8()?, "42,alice,");

    Ok(())
}

#[test]
fn route_macros() -> izanami::Result<()> {
    let app = App::create(chain![
        path!("/root") //
            .to(endpoint::call(|| "root")),
        path!("/params/:id/:name") //
            .to(endpoint::call(|id: i32, name: String| format!(
                "params(id={}, name={})",
                id, name
            ))),
        path!("/posts/:id/edit") //
            .to({
                endpoint::put()
                    .extract(extractor::body::plain::<String>())
                    .call(|id: u32, body: String| format!("posts(id={}, body={})", id, body))
            }),
        path!("/static/*path") //
            .to(endpoint::call(|path: String| format!(
                "static(path={})",
                path
            ))),
    ])?;
    let mut server = izanami::test::server(app)?;

    let response = server.perform("/root")?;
    assert_eq!(response.body().to_utf8()?, "root");

    let response = server.perform("/params/42/alice")?;
    assert_eq!(response.body().to_utf8()?, "params(id=42, name=alice)");

    let response = server.perform(
        Request::put("/posts/1/edit")
            .header("content-type", "text/plain; charset=utf-8")
            .body("fox"),
    )?;
    assert_eq!(response.body().to_utf8()?, "posts(id=1, body=fox)");

    let response = server.perform("/static/path/to/file.txt")?;
    assert_eq!(response.body().to_utf8()?, "static(path=path/to/file.txt)");

    Ok(())
}

#[test]
fn plain_body() -> izanami::Result<()> {
    let app = App::create(
        path!("/") //
            .to(endpoint::post()
                .extract(extractor::body::plain())
                .call(|body: String| body)),
    )?;
    let mut server = izanami::test::server(app)?;

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
fn json_body() -> izanami::Result<()> {
    #[derive(Debug, serde::Deserialize)]
    struct Params {
        id: u32,
        name: String,
    }

    let app = App::create(
        path!("/") //
            .to(endpoint::post()
                .extract(extractor::body::json())
                .call(|params: Params| format!("{},{}", params.id, params.name))),
    )?;
    let mut server = izanami::test::server(app)?;

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
fn urlencoded_body() -> izanami::Result<()> {
    #[derive(Debug, serde::Deserialize)]
    struct Params {
        id: u32,
        name: String,
    }

    let app = App::create(
        path!("/") //
            .to(endpoint::post()
                .extract(extractor::body::urlencoded())
                .call(|params: Params| format!("{},{}", params.id, params.name))),
    )?;
    let mut server = izanami::test::server(app)?;

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
fn local_data() -> izanami::Result<()> {
    use {
        futures01::Poll,
        tsukuyomi::{
            future::TryFuture,
            handler::{Handler, Metadata, ModifyHandler},
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

        fn metadata(&self) -> Metadata {
            self.0.metadata()
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

    impl<H: TryFuture> TryFuture for InsertMyDataHandle<H> {
        type Ok = H::Ok;
        type Error = H::Error;

        fn poll_ready(&mut self, input: &mut Input) -> Poll<Self::Ok, Self::Error> {
            input
                .locals
                .entry(&MyData::KEY)
                .or_insert_with(|| MyData("dummy".into()));
            self.handle.poll_ready(input)
        }
    }

    let app = App::create(
        path!("/") //
            .to({
                endpoint::any()
                    .extract(extractor::local::remove(&MyData::KEY))
                    .call(|x: MyData| x.0)
            })
            .modify(InsertMyData::default()),
    )?;
    let mut server = izanami::test::server(app)?;

    let response = server.perform("/")?;
    assert_eq!(response.status(), 200);

    Ok(())
}

#[test]
fn missing_local_data() -> izanami::Result<()> {
    use tsukuyomi::input::localmap::local_key;

    #[derive(Clone)]
    struct MyData(String);

    impl MyData {
        local_key! {
            const KEY: Self;
        }
    }

    let app = App::create({
        path!("/") //
            .to(endpoint::any()
                .extract(extractor::local::remove(&MyData::KEY))
                .call(|x: MyData| x.0))
    })?;
    let mut server = izanami::test::server(app)?;

    let response = server.perform("/")?;
    assert_eq!(response.status(), 500);

    Ok(())
}

#[test]
fn optional() -> izanami::Result<()> {
    #[derive(Debug, serde::Deserialize)]
    struct Params {
        id: u32,
        name: String,
    }

    let extractor = extractor::body::json().optional();

    let app = App::create(
        path!("/") //
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
    let mut server = izanami::test::server(app)?;

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

    Ok(())
}

#[test]
fn either_or() -> izanami::Result<()> {
    #[derive(Debug, serde::Deserialize)]
    struct Params {
        id: u32,
        name: String,
    }

    let params_extractor = (extractor::method::get().and(extractor::query()))
        .or(extractor::method::post().and(extractor::body::json()))
        .or(extractor::method::post().and(extractor::body::urlencoded()));

    let app = App::create(
        path!("/") //
            .to({
                endpoint::allow_only("GET, POST")?
                    .extract(params_extractor)
                    .call(|params: Params| format!("{},{}", params.id, params.name))
            }),
    )?;
    let mut server = izanami::test::server(app)?;

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
