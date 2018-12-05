use {
    http::Request,
    tsukuyomi::{
        app::{route, App}, //
        extractor,
        Extractor,
    },
};

#[test]
fn unit_input() -> tsukuyomi::test::Result<()> {
    let mut server = App::builder()
        .with(route::root().reply(|| "dummy"))
        .build_server()?
        .into_test_server()?;
    let response = server.perform("/")?;
    assert_eq!(response.status(), 200);
    Ok(())
}

#[test]
fn params() -> tsukuyomi::test::Result<()> {
    let mut server = App::builder()
        .with(
            route::root()
                .param("id")?
                .param("name")?
                .catch_all("path")?
                .reply(|id: u32, name: String, path: String| format!("{},{},{}", id, name, path)),
        ) //
        .build_server()?
        .into_test_server()?;

    let response = server.perform("/23/bob/path/to/file")?;
    assert_eq!(response.body().to_utf8()?, "23,bob,path/to/file");

    let response = server.perform("/42/alice/")?;
    assert_eq!(response.body().to_utf8()?, "42,alice,");

    Ok(())
}

#[test]
fn route_macros() -> tsukuyomi::test::Result<()> {
    drop(
        App::builder()
            .with(route::root().segment("root")?.reply(|| "root"))
            .with(
                route::root()
                    .segment("params")?
                    .param("id")?
                    .param("name")?
                    .reply(|id: i32, name: String| {
                        drop((id, name));
                        "dummy"
                    }),
            ) //
            .with(
                route::root()
                    .segment("posts")?
                    .param("id")?
                    .segment("edit")?
                    .methods("PUT")?
                    .extract(extractor::body::plain::<String>())
                    .reply(|id: u32, body: String| {
                        drop((id, body));
                        "dummy"
                    }),
            ) //
            .with(
                route::root()
                    .segment("static")?
                    .catch_all("path")?
                    .reply(|path: String| {
                        drop(path);
                        "dummy"
                    }),
            ).build()?,
    );

    Ok(())
}

#[test]
fn plain_body() -> tsukuyomi::test::Result<()> {
    let mut server = App::builder()
        .with(
            route::root()
                .methods("POST")?
                .extract(extractor::body::plain())
                .reply(|body: String| body),
        ) //
        .build_server()?
        .into_test_server()?;

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

    let mut server = App::builder()
        .with(
            route::root()
                .methods("POST")?
                .extract(extractor::body::json())
                .reply(|params: Params| format!("{},{}", params.id, params.name)),
        ) //
        .build_server()?
        .into_test_server()?;

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

    let mut server = App::builder()
        .with(
            route::root()
                .methods("POST")?
                .extract(extractor::body::urlencoded())
                .reply(|params: Params| format!("{},{}", params.id, params.name)),
        ) //
        .build_server()?
        .into_test_server()?;

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
    use tsukuyomi::{
        handler::{Handler, ModifyHandler},
        localmap::local_key,
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
        type Error = H::Error;
        type Handler = InsertMyDataHandler<H>;

        fn modify(&self, inner: H) -> Self::Handler {
            InsertMyDataHandler(inner)
        }
    }

    struct InsertMyDataHandler<H>(H);

    impl<H: Handler> Handler for InsertMyDataHandler<H> {
        type Output = H::Output;
        type Error = H::Error;
        type Future = H::Future;

        fn call(&self, input: &mut tsukuyomi::Input<'_>) -> tsukuyomi::MaybeFuture<Self::Future> {
            input.locals.insert(&MyData::KEY, MyData("dummy".into()));
            self.0.call(input)
        }
    }

    let mut server = App::builder()
        .modifier(InsertMyData::default())
        .with(
            route::root()
                .extract(extractor::local::remove(&MyData::KEY))
                .reply(|x: MyData| x.0),
        ) //
        .build_server()?
        .into_test_server()?;

    let response = server.perform("/")?;
    assert_eq!(response.status(), 200);

    Ok(())
}

#[test]
fn missing_local_data() -> tsukuyomi::test::Result<()> {
    use tsukuyomi::localmap::local_key;

    #[derive(Clone)]
    struct MyData(String);

    impl MyData {
        local_key! {
            const KEY: Self;
        }
    }

    let mut server = App::builder()
        .with(
            route::root()
                .extract(extractor::local::remove(&MyData::KEY))
                .reply(|x: MyData| x.0),
        ) //
        .build_server()?
        .into_test_server()?;

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

    let extractor = extractor::Builder::new(extractor::body::json()).optional();

    let mut server = App::builder()
        .with(
            route::root()
                .methods("POST")?
                .extract(extractor) //
                .call(|params: Option<Params>| {
                    if let Some(params) = params {
                        Ok(format!("{},{}", params.id, params.name))
                    } else {
                        Err(tsukuyomi::error::internal_server_error("####none####"))
                    }
                }),
        ) //
        .build_server()?
        .into_test_server()?;

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

    let params_extractor = extractor::verb::get(extractor::query::query())
        .into_builder()
        .or(extractor::verb::post(extractor::body::json()))
        .or(extractor::verb::post(extractor::body::urlencoded()));

    let mut server = App::builder()
        .with(
            route::root()
                .methods("POST")?
                .extract(params_extractor)
                .reply(|params: Params| format!("{},{}", params.id, params.name)),
        ) //
        .build_server()?
        .into_test_server()?;

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
