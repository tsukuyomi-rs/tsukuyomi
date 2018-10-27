use tsukuyomi::app::App;
use tsukuyomi::extract::body::{Json, Plain, Urlencoded};
use tsukuyomi::handler::with_extractor;
use tsukuyomi::input::local_map::LocalData;

use either::Either;
use http::Request;

use super::util::{local_server, LocalServerExt};

#[test]
fn unit_input() {
    let mut server = local_server(App::builder().route(("/", with_extractor((), || Ok("dummy")))));

    let response = server.perform(Request::get("/")).unwrap();
    assert_eq!(response.status().as_u16(), 200);
}

#[test]
fn params() {
    use tsukuyomi::extract::param::{Named, Pos, Wildcard};

    let mut server = local_server(App::builder().route((
        "/:id/:name/*path",
        with_extractor(
            (Pos::new(0), Named::new("name"), Wildcard::new()),
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
        with_extractor((Plain::<String>::default(),), Ok),
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
        with_extractor((Json::default(),), |params: Params| {
            Ok(format!("{},{}", params.id, params.name))
        }),
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
        with_extractor((Urlencoded::default(),), |params: Params| {
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
                .body(r#"THIS_IS_INVALID_FORM_DATA"#.as_bytes()),
        ).unwrap();
    assert_eq!(response.status().as_u16(), 400);
}

#[test]
fn local_data() {
    use tsukuyomi::extract::{Directly, Local};

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
            with_extractor((Directly::default(),), |foo: Local<Foo>| {
                Ok(foo.with(Clone::clone).0)
            }),
        ))
    });

    let response = server.perform(Request::get("/")).unwrap();
    assert_eq!(response.status().as_u16(), 200);
}

#[test]
fn missing_local_data() {
    use tsukuyomi::extract::{Directly, Local};

    #[derive(Clone, LocalData)]
    struct Foo(String);

    let mut server = local_server(App::builder().route((
        "/",
        with_extractor((Directly::default(),), |foo: Local<Foo>| {
            Ok(foo.with(Clone::clone).0)
        }),
    )));

    let response = server.perform(Request::get("/")).unwrap();
    assert_eq!(response.status().as_u16(), 500);
}

#[test]
fn either() {
    use tsukuyomi::extract::EitherOf;

    #[derive(Debug, serde::Deserialize)]
    struct Params {
        id: u32,
        name: String,
    }

    let mut server = local_server(App::builder().route((
        "/",
        "POST",
        with_extractor(
            (EitherOf::new(Json::default(), Urlencoded::default()),),
            |params: Either<Params, Params>| {
                let params = params.into_inner();
                Ok(format!("{},{}", params.id, params.name))
            },
        ),
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
