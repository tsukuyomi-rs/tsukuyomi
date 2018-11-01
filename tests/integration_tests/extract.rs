use tsukuyomi::app::App;
use tsukuyomi::extractor;
use tsukuyomi::extractor::ExtractorExt;
use tsukuyomi::route;

use http::Request;

use super::util::{local_server, LocalServerExt};

#[test]
fn unit_input() {
    let mut server = local_server(App::builder().route(route::index().reply(|| "dummy")));

    let response = server.perform(Request::get("/")).unwrap();
    assert_eq!(response.status().as_u16(), 200);
}

#[test]
fn params() {
    use tsukuyomi::extractor::param;

    let mut server = local_server(
        App::builder().route(
            route::get("/:id/:name/*path")
                .with(param::pos(0))
                .with(param::named("name"))
                .with(param::wildcard())
                .reply(|id: u32, name: String, path: String| format!("{},{},{}", id, name, path)),
        ),
    );

    let response = server
        .perform(Request::get("/23/bob/path/to/file"))
        .unwrap();
    assert_eq!(response.body().to_utf8().unwrap(), "23,bob,path/to/file");

    let response = server.perform(Request::get("/42/alice/")).unwrap();
    assert_eq!(response.body().to_utf8().unwrap(), "42,alice,");
}

#[test]
#[ignore]
fn route_macros() {
    let app = App::builder()
        .route(tsukuyomi::get!("/index").reply(|| "index"))
        .route(
            tsukuyomi::get!("/params/<id:u32>/<name:String>").reply(|id, name| {
                drop((id, name));
                "dummy"
            }),
        ).route(
            tsukuyomi::put!("/posts/<id:u32>/edit")
                .with(extractor::body::plain::<String>())
                .reply(|id: u32, body: String| {
                    drop((id, body));
                    "dummy"
                }),
        ).finish()
        .unwrap();
    drop(app);
}

#[test]
fn plain_body() {
    let mut server = local_server(
        App::builder().route(
            route::post("/")
                .with(extractor::body::plain())
                .reply(|body: String| body),
        ),
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
    let mut server = local_server(
        App::builder().route(
            route::post("/")
                .with(extractor::body::json())
                .reply(|params: Params| format!("{},{}", params.id, params.name)),
        ),
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
    let mut server = local_server(
        App::builder().route(
            route::post("/")
                .with(extractor::body::urlencoded())
                .reply(|params: Params| format!("{},{}", params.id, params.name)),
        ),
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
    use tsukuyomi::input::local_map::local_key;
    use tsukuyomi::input::Input;
    use tsukuyomi::modifier::{BeforeHandle, Modifier};

    #[derive(Clone)]
    struct MyData(String);

    impl MyData {
        local_key!(const KEY: Self);
    }

    struct MyModifier;
    impl Modifier for MyModifier {
        fn before_handle(&self, input: &mut Input<'_>) -> BeforeHandle {
            input
                .locals_mut()
                .insert(&MyData::KEY, MyData("dummy".into()));
            BeforeHandle::ready(Ok(None))
        }
    }

    let mut server = local_server({
        App::builder().modifier(MyModifier).route(
            route::index()
                .with(extractor::local(&MyData::KEY))
                .reply(|x: MyData| x.0),
        )
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

    let mut server = local_server(
        App::builder().route(
            route::index()
                .with(extractor::local(&MyData::KEY))
                .reply(|x: MyData| x.0),
        ),
    );

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

    let mut server = local_server(
        App::builder().route(
            route::post("/")
                .with(extractor::body::json().optional())
                .handle(|params: Option<Params>| {
                    if let Some(params) = params {
                        Ok(format!("{},{}", params.id, params.name))
                    } else {
                        Err(tsukuyomi::error::internal_server_error("####none####").into())
                    }
                }),
        ),
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
fn fallible() {
    #[derive(Debug, serde::Deserialize)]
    struct Params {
        id: u32,
        name: String,
    }

    let mut server = local_server(
        App::builder().route(
            route::post("/")
                .with(extractor::body::json().fallible())
                .handle(|params: Result<Params, _>| {
                    if let Ok(params) = params {
                        Ok(format!("{},{}", params.id, params.name))
                    } else {
                        Err(tsukuyomi::error::internal_server_error("####err####").into())
                    }
                }),
        ),
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
    assert_eq!(response.body().to_utf8().unwrap(), "####err####");
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

    let mut server = local_server(
        App::builder().route(
            route::post("/")
                .with(params_extractor)
                .reply(|params: Params| format!("{},{}", params.id, params.name)),
        ),
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
