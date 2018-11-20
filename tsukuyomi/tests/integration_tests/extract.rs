use {
    http::Request,
    tsukuyomi::{
        extractor::{self, Extractor},
        route,
    },
};

#[test]
fn unit_input() -> tsukuyomi::test::Result<()> {
    let mut server = tsukuyomi::app!()
        .route(route!().reply(|| "dummy"))
        .build_server()?
        .into_test_server()?;
    let response = server.perform("/")?;
    assert_eq!(response.status(), 200);
    Ok(())
}

#[test]
fn params() -> tsukuyomi::test::Result<()> {
    let mut server = tsukuyomi::app!()
        .route(
            route!("/:id/:name/*path")
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
        tsukuyomi::app!()
            .route(route!("/index").reply(|| "index"))
            .route(route!("/params/:id/:name").reply(|id: i32, name: String| {
                drop((id, name));
                "dummy"
            })) //
            .route(
                route!("/posts/:id/edit", method = PUT)
                    .with(extractor::body::plain::<String>())
                    .reply(|id: u32, body: String| {
                        drop((id, body));
                        "dummy"
                    }),
            ) //
            .route(route!("/static/*path").reply(|path: String| {
                drop(path);
                "dummy"
            })).build()?,
    );

    Ok(())
}

#[test]
fn plain_body() -> tsukuyomi::test::Result<()> {
    let mut server = tsukuyomi::app!()
        .route(
            route!("/", method = POST)
                .with(extractor::body::plain())
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

    let mut server = tsukuyomi::app!()
        .route(
            route!("/", method = POST)
                .with(extractor::body::json())
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

    let mut server = tsukuyomi::app!()
        .route(
            route!("/", method = POST)
                .with(extractor::body::urlencoded())
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
    use tsukuyomi::{handler::AsyncResult, localmap::local_key, Modifier, Output};

    #[derive(Clone)]
    struct MyData(String);

    impl MyData {
        local_key! {
            const KEY: Self;
        }
    }

    struct MyModifier;
    impl Modifier for MyModifier {
        fn modify(&self, mut result: AsyncResult<Output>) -> AsyncResult<Output> {
            let mut inserted = false;
            AsyncResult::poll_fn(move |input| {
                if !inserted {
                    input.locals.insert(&MyData::KEY, MyData("dummy".into()));
                    inserted = true;
                }
                result.poll_ready(input)
            })
        }
    }

    let mut server = tsukuyomi::app!()
        .modifier(MyModifier)
        .route(
            route!()
                .with(extractor::local::remove(&MyData::KEY))
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

    let mut server = tsukuyomi::app!()
        .route(
            route!()
                .with(extractor::local::remove(&MyData::KEY))
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

    let mut server = tsukuyomi::app!()
        .route(
            route!("/", method = POST)
                .with(extractor)
                .handle(|params: Option<Params>| {
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

    let mut server = tsukuyomi::app!()
        .route(
            route!("/", method = POST)
                .with(params_extractor)
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
