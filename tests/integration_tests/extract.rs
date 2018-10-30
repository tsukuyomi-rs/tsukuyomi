use tsukuyomi::app::App;
use tsukuyomi::extractor::body::{Json, Plain, Urlencoded};
use tsukuyomi::extractor::ExtractorExt;
use tsukuyomi::handler;

use either::Either;
use http::Request;

use super::util::{local_server, LocalServerExt};

#[test]
fn unit_input() {
    let mut server =
        local_server(App::builder().route(("/", handler::extract((), || Ok("dummy")))));

    let response = server.perform(Request::get("/")).unwrap();
    assert_eq!(response.status().as_u16(), 200);
}

#[test]
fn params() {
    use tsukuyomi::extractor::param::{Named, Pos, Wildcard};

    let mut server = local_server(App::builder().route((
        "/:id/:name/*path",
        handler::extract(
            Pos::new(0).and(Named::new("name")).and(Wildcard::new()),
            |id: u32, name: String, path: String| Ok(format!("{},{},{}", id, name, path)),
        ),
    )));

    let response = server
        .perform(Request::get("/23/bob/path/to/file"))
        .unwrap();
    assert_eq!(response.body().to_utf8().unwrap(), "23,bob,path/to/file");

    let response = server.perform(Request::get("/42/alice/")).unwrap();
    assert_eq!(response.body().to_utf8().unwrap(), "42,alice,");
}

#[test]
fn plain_body() {
    let mut server = local_server(App::builder().route((
        "/",
        "POST",
        handler::extract(Plain::<String>::default(), Ok),
    )));

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
    let mut server = local_server(App::builder().route((
        "/",
        "POST",
        handler::extract(Json::default(), |params: Params| {
            Ok(format!("{},{}", params.id, params.name))
        }),
    )));

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
    let mut server = local_server(App::builder().route((
        "/",
        "POST",
        handler::extract(Urlencoded::default(), |params: Params| {
            Ok(format!("{},{}", params.id, params.name))
        }),
    )));

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
    use tsukuyomi::extractor::LocalExtractor;
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
        App::builder().modifier(MyModifier).route((
            "/",
            handler::extract(LocalExtractor::new(&MyData::KEY), |x: MyData| Ok(x.0)),
        ))
    });

    let response = server.perform(Request::get("/")).unwrap();
    assert_eq!(response.status().as_u16(), 200);
}

#[test]
fn missing_local_data() {
    use tsukuyomi::extractor::LocalExtractor;
    use tsukuyomi::input::local_map::local_key;

    #[derive(Clone)]
    struct MyData(String);

    impl MyData {
        local_key!(const KEY: Self);
    }

    let mut server = local_server(App::builder().route((
        "/",
        handler::extract(LocalExtractor::new(&MyData::KEY), |x: MyData| Ok(x.0)),
    )));

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

    let extractor = Json::default().optional();

    let mut server = local_server(App::builder().route((
        "/",
        "POST",
        handler::extract(extractor, |params: Option<Params>| {
            if let Some(params) = params {
                Ok(format!("{},{}", params.id, params.name))
            } else {
                Err(tsukuyomi::error::internal_server_error("####none####").into())
            }
        }),
    )));

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

    let extractor = Json::default().fallible();

    let mut server = local_server(App::builder().route((
        "/",
        "POST",
        handler::extract(extractor, |params: Result<Params, _>| {
            if let Ok(params) = params {
                Ok(format!("{},{}", params.id, params.name))
            } else {
                Err(tsukuyomi::error::internal_server_error("####err####").into())
            }
        }),
    )));

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

    let extractor = Json::default().or(Urlencoded::default());

    let mut server = local_server(App::builder().route((
        "/",
        "POST",
        handler::extract(extractor, |params: Either<Params, Params>| {
            let params = params.into_inner();
            Ok(format!("{},{}", params.id, params.name))
        }),
    )));

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
