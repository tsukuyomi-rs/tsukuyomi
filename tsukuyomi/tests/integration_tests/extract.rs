use tsukuyomi::app::route;
use tsukuyomi::extractor;
use tsukuyomi::extractor::ExtractorExt;
use tsukuyomi::test::test_server;

use http::Request;

#[test]
fn unit_input() {
    let mut server = test_server(
        tsukuyomi::app()
            .route(route!().reply(|| "dummy"))
            .finish()
            .unwrap(),
    );
    let response = server.perform(Request::get("/")).unwrap();
    assert_eq!(response.status().as_u16(), 200);
}

#[test]
fn params() {
    let mut server = test_server(
        tsukuyomi::app()
            .route(
                route!("/:id/:name/*path").reply(|id: u32, name: String, path: String| {
                    format!("{},{},{}", id, name, path)
                }),
            ) //
            .finish()
            .unwrap(),
    );

    let response = server
        .perform(Request::get("/23/bob/path/to/file"))
        .unwrap();
    assert_eq!(response.body().to_utf8().unwrap(), "23,bob,path/to/file");

    let response = server.perform(Request::get("/42/alice/")).unwrap();
    assert_eq!(response.body().to_utf8().unwrap(), "42,alice,");
}

#[test]
fn route_macros() {
    drop(
        tsukuyomi::app()
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
            })).finish()
            .unwrap(),
    );
}

#[test]
fn plain_body() {
    let mut server = test_server(
        tsukuyomi::app()
            .route(
                route!("/", method = POST)
                    .with(extractor::body::plain())
                    .reply(|body: String| body),
            ) //
            .finish()
            .unwrap(),
    );

    const BODY: &[u8] = b"The quick brown fox jumps over the lazy dog";

    let response = server
        .perform(
            Request::post("/")
                .header("content-type", "text/plain; charset=utf-8")
                .body(BODY),
        ).unwrap();
    assert_eq!(response.status().as_u16(), 200);

    // missing content-type
    let response = server.perform(Request::post("/").body(BODY)).unwrap();
    assert_eq!(response.status().as_u16(), 200);

    // invalid content-type
    let response = server
        .perform(
            Request::post("/")
                .header("content-type", "application/graphql")
                .body(BODY),
        ).unwrap();
    assert_eq!(response.status().as_u16(), 400);

    // invalid charset
    let response = server
        .perform(
            Request::post("/")
                .header("content-type", "text/plain; charset=euc-jp")
                .body(BODY),
        ).unwrap();
    assert_eq!(response.status().as_u16(), 400);
}

#[test]
fn json_body() {
    #[derive(Debug, serde::Deserialize)]
    struct Params {
        id: u32,
        name: String,
    }
    let mut server = test_server(
        tsukuyomi::app()
            .route(
                route!("/", method = POST)
                    .with(extractor::body::json())
                    .reply(|params: Params| format!("{},{}", params.id, params.name)),
            ) //
            .finish()
            .unwrap(),
    );

    let response = server
        .perform(
            Request::post("/")
                .header("content-type", "application/json")
                .body(&br#"{"id":23, "name":"bob"}"#[..]),
        ).unwrap();
    assert_eq!(response.body().to_utf8().unwrap(), "23,bob");

    // missing content-type
    let response = server
        .perform(Request::post("/").body(&br#"{"id":23, "name":"bob"}"#[..]))
        .unwrap();
    assert_eq!(response.status().as_u16(), 400);

    // invalid content-type
    let response = server
        .perform(
            Request::post("/")
                .header("content-type", "application/graphql")
                .body(&br#"{"id":23, "name":"bob"}"#[..]),
        ).unwrap();
    assert_eq!(response.status().as_u16(), 400);

    // invalid data
    let response = server
        .perform(
            Request::post("/")
                .header("content-type", "application/json")
                .body(&br#"THIS_IS_INVALID_JSON_DATA"#[..]),
        ).unwrap();
    assert_eq!(response.status().as_u16(), 400);
}

#[test]
fn urlencoded_body() {
    #[derive(Debug, serde::Deserialize)]
    struct Params {
        id: u32,
        name: String,
    }
    let mut server = test_server(
        tsukuyomi::app()
            .route(
                route!("/", method = POST)
                    .with(extractor::body::urlencoded())
                    .reply(|params: Params| format!("{},{}", params.id, params.name)),
            ) //
            .finish()
            .unwrap(),
    );

    const BODY: &[u8] = b"id=23&name=bob";

    let response = server
        .perform(
            Request::post("/")
                .header("content-type", "application/x-www-form-urlencoded")
                .body(BODY),
        ).unwrap();
    assert_eq!(response.body().to_utf8().unwrap(), "23,bob");

    // missing content-type
    let response = server.perform(Request::post("/").body(BODY)).unwrap();
    assert_eq!(response.status().as_u16(), 400);

    // invalid content-type
    let response = server
        .perform(
            Request::post("/")
                .header("content-type", "application/graphql")
                .body(BODY),
        ).unwrap();
    assert_eq!(response.status().as_u16(), 400);

    // invalid data
    let response = server
        .perform(
            Request::post("/")
                .header("content-type", "application/x-www-form-urlencoded")
                .body(&br#"THIS_IS_INVALID_FORM_DATA"#[..]),
        ).unwrap();
    assert_eq!(response.status().as_u16(), 400);
}

#[test]
fn local_data() {
    use tsukuyomi::app::{AsyncResult, Modifier};
    use tsukuyomi::input::local_map::local_key;
    use tsukuyomi::input::Input;
    use tsukuyomi::output::Output;

    #[derive(Clone)]
    struct MyData(String);

    impl MyData {
        local_key!(const KEY: Self);
    }

    struct MyModifier;
    impl Modifier for MyModifier {
        fn before_handle(&self, input: &mut Input<'_>) -> AsyncResult<Option<Output>> {
            input
                .locals_mut()
                .insert(&MyData::KEY, MyData("dummy".into()));
            AsyncResult::ready(Ok(None))
        }
    }

    let mut server = test_server({
        tsukuyomi::app()
            .modifier(MyModifier)
            .route(
                route!()
                    .with(extractor::local::remove(&MyData::KEY))
                    .reply(|x: MyData| x.0),
            ) //
            .finish()
            .unwrap()
    });

    let response = server.perform(Request::get("/")).unwrap();
    assert_eq!(response.status().as_u16(), 200);
}

#[test]
fn missing_local_data() {
    use tsukuyomi::input::local_map::local_key;

    #[derive(Clone)]
    struct MyData(String);

    impl MyData {
        local_key!(const KEY: Self);
    }

    let mut server = test_server({
        tsukuyomi::app()
            .route(
                route!()
                    .with(extractor::local::remove(&MyData::KEY))
                    .reply(|x: MyData| x.0),
            ) //
            .finish()
            .unwrap()
    });

    let response = server.perform(Request::get("/")).unwrap();
    assert_eq!(response.status().as_u16(), 500);
}

#[test]
fn optional() {
    #[derive(Debug, serde::Deserialize)]
    struct Params {
        id: u32,
        name: String,
    }

    let mut server = test_server(
        tsukuyomi::app()
            .route(
                route!("/", method = POST)
                    .with(extractor::body::json().optional())
                    .handle(|params: Option<Params>| {
                        if let Some(params) = params {
                            Ok(format!("{},{}", params.id, params.name))
                        } else {
                            Err(tsukuyomi::error::internal_server_error("####none####"))
                        }
                    }),
            ) //
            .finish()
            .unwrap(),
    );

    let response = server
        .perform(
            Request::post("/")
                .header("content-type", "application/json")
                .body(&br#"{"id":23, "name":"bob"}"#[..]),
        ).unwrap();
    assert_eq!(response.status().as_u16(), 200);
    assert_eq!(response.body().to_utf8().unwrap(), "23,bob");

    let response = server
        .perform(
            Request::post("/")
                .header("content-type", "application/x-www-form-urlencoded")
                .body(&b"id=23&name=bob"[..]),
        ).unwrap();
    assert_eq!(response.status().as_u16(), 500);
    assert_eq!(response.body().to_utf8().unwrap(), "####none####");
}

#[test]
fn either_or() {
    #[derive(Debug, serde::Deserialize)]
    struct Params {
        id: u32,
        name: String,
    }

    let params_extractor = extractor::verb::get(extractor::query::query())
        .or(extractor::verb::post(extractor::body::json()))
        .or(extractor::verb::post(extractor::body::urlencoded()));

    let mut server = test_server(
        tsukuyomi::app()
            .route(
                route!("/", method = POST)
                    .with(params_extractor)
                    .reply(|params: Params| format!("{},{}", params.id, params.name)),
            ) //
            .finish()
            .unwrap(),
    );

    let response = server
        .perform(
            Request::post("/")
                .header("content-type", "application/json")
                .body(&br#"{"id":23, "name":"bob"}"#[..]),
        ).unwrap();
    assert_eq!(response.status().as_u16(), 200);
    assert_eq!(response.body().to_utf8().unwrap(), "23,bob");

    let response = server
        .perform(
            Request::post("/")
                .header("content-type", "application/x-www-form-urlencoded")
                .body(&b"id=23&name=bob"[..]),
        ).unwrap();
    assert_eq!(response.status().as_u16(), 200);
    assert_eq!(response.body().to_utf8().unwrap(), "23,bob");

    let response = server
        .perform(
            Request::post("/")
                .header("content-type", "text/plain; charset=utf-8")
                .body(&b"///invalid string"[..]),
        ).unwrap();
    assert_eq!(response.status().as_u16(), 400);
}
