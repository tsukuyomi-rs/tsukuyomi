use tsukuyomi::app::App;
use tsukuyomi::extract::body::{Json, Plain, Urlencoded};
use tsukuyomi::extract::param::{Param, Wildcard};
use tsukuyomi::extract::Local;
use tsukuyomi::handler;
use tsukuyomi::input::local_map::LocalData;

use either::Either;
use http::Request;

use super::util::{local_server, LocalServerExt};

#[test]
fn unit_input() {
    let mut server = local_server(App::builder().route(("/", handler::extract_ready(|| "dummy"))));

    let response = server.perform(Request::get("/")).unwrap();
    assert_eq!(response.status().as_u16(), 200);
}

#[test]
fn params() {
    let mut server = local_server(App::builder().route((
        "/:id/:name/*path",
        handler::extract_ready(
            |id: Param<u32>, name: Param<String>, path: Wildcard<String>| {
                format!("{},{},{}", &*id, &*name, &*path)
            },
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
        handler::extract_ready(|body: Plain<String>| body.into_inner()),
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
        handler::extract_ready(|params: Json<Params>| format!("{},{}", params.id, params.name)),
    )));

    let response = server
        .perform(
            Request::post("/")
                .header("content-type", "application/json")
                .body(r#"{"id":23, "name":"bob"}"#.as_bytes()),
        ).unwrap();
    assert_eq!(response.body().to_utf8().unwrap(), "23,bob");

    // missing content-type
    let response = server
        .perform(Request::post("/").body(r#"{"id":23, "name":"bob"}"#.as_bytes()))
        .unwrap();
    assert_eq!(response.status().as_u16(), 400);

    // invalid content-type
    let response = server
        .perform(
            Request::post("/")
                .header("content-type", "application/graphql")
                .body(r#"{"id":23, "name":"bob"}"#.as_bytes()),
        ).unwrap();
    assert_eq!(response.status().as_u16(), 400);

    // invalid data
    let response = server
        .perform(
            Request::post("/")
                .header("content-type", "application/json")
                .body(r#"THIS_IS_INVALID_JSON_DATA"#.as_bytes()),
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
        handler::extract_ready(|params: Urlencoded<Params>| {
            format!("{},{}", params.id, params.name)
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
                .body(r#"THIS_IS_INVALID_FORM_DATA"#.as_bytes()),
        ).unwrap();
    assert_eq!(response.status().as_u16(), 400);
}

#[test]
fn local_data() {
    #[derive(Clone, LocalData)]
    struct Foo(String);

    use tsukuyomi::input::Input;
    use tsukuyomi::modifier::{BeforeHandle, Modifier};
    struct MyModifier;
    impl Modifier for MyModifier {
        fn before_handle(&self, input: &mut Input<'_>) -> BeforeHandle {
            Foo("dummy".into()).insert_into(input.locals_mut());
            BeforeHandle::ready(Ok(None))
        }
    }

    let mut server = local_server({
        App::builder().modifier(MyModifier).route((
            "/",
            handler::extract_ready(|foo: Local<Foo>| foo.with(Clone::clone).0),
        ))
    });

    let response = server.perform(Request::get("/")).unwrap();
    assert_eq!(response.status().as_u16(), 200);
}

#[test]
fn missing_local_data() {
    #[derive(Clone, LocalData)]
    struct Foo(String);

    let mut server = local_server(App::builder().route((
        "/",
        handler::extract_ready(|foo: Local<Foo>| foo.with(Clone::clone).0),
    )));

    let response = server.perform(Request::get("/")).unwrap();
    assert_eq!(response.status().as_u16(), 500);
}

#[test]
fn either() {
    #[derive(Debug, serde::Deserialize)]
    struct Params {
        id: u32,
        name: String,
    }

    let mut server = local_server(App::builder().route((
        "/",
        "POST",
        handler::extract_ready(|params: Either<Json<Params>, Urlencoded<Params>>| {
            let params = match params {
                Either::Left(Json(params)) => params,
                Either::Right(Urlencoded(params)) => params,
            };
            format!("{},{}", params.id, params.name)
        }),
    )));

    let response = server
        .perform(
            Request::post("/")
                .header("content-type", "application/json")
                .body(r#"{"id":23, "name":"bob"}"#.as_bytes()),
        ).unwrap();
    assert_eq!(response.status().as_u16(), 200);
    assert_eq!(response.body().to_utf8().unwrap(), "23,bob");

    let response = server
        .perform(
            Request::post("/")
                .header("content-type", "application/x-www-form-urlencoded")
                .body("id=23&name=bob".as_bytes()),
        ).unwrap();
    assert_eq!(response.status().as_u16(), 200);
    assert_eq!(response.body().to_utf8().unwrap(), "23,bob");

    let response = server
        .perform(
            Request::post("/")
                .header("content-type", "text/plain; charset=utf-8")
                .body("///invalid string".as_bytes()),
        ).unwrap();
    assert_eq!(response.status().as_u16(), 400);
}
