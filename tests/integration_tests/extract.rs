use tsukuyomi::app::App;
use tsukuyomi::extract::body::{Json, Plain, Urlencoded};
use tsukuyomi::extract::param::{Param, Wildcard};
use tsukuyomi::extract::Local;
use tsukuyomi::handler::wrap_async;
use tsukuyomi::input::local_map::LocalData;

use either::Either;
use futures::prelude::*;
use http::Request;

use super::util::{local_server, LocalServerExt};

#[test]
fn unit_input() {
    let mut server = local_server(App::builder().route((
        "/",
        wrap_async(|input| input.extract::<()>().map(|_| "dummy")),
    )));

    let response = server.perform(Request::get("/")).unwrap();
    assert_eq!(response.status().as_u16(), 200);
}

#[test]
fn params() {
    let mut server = local_server(App::builder().route((
        "/:id/:name/*path",
        wrap_async(|input| {
            input
                .extract::<(Param<u32>, Param<String>, Wildcard<String>)>()
                .map(|(id, name, path)| format!("{},{},{}", &*id, &*name, &*path))
        }),
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
        wrap_async(|input| {
            input
                .extract::<Plain<String>>()
                .map(|body| body.into_inner())
        }),
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
    let mut server = local_server(App::builder().route((
        "/",
        "POST",
        wrap_async(|input| {
            #[derive(Debug, serde::Deserialize)]
            struct Params {
                id: u32,
                name: String,
            }
            input
                .extract::<Json<Params>>()
                .map(|params| format!("{},{}", params.id, params.name))
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
    let mut server = local_server(App::builder().route((
        "/",
        "POST",
        wrap_async(|input| {
            #[derive(Debug, serde::Deserialize)]
            struct Params {
                id: u32,
                name: String,
            }
            input
                .extract::<Urlencoded<Params>>()
                .map(|params| format!("{},{}", params.id, params.name))
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
    #[derive(LocalData)]
    struct Foo(String);

    let mut server = local_server(App::builder().route((
        "/",
        wrap_async(|input| {
            Foo("dummy".into()).insert_into(input.locals_mut());
            input
                .extract::<Local<Foo>>()
                .map(|locals| locals.with(|data| data.0.clone()))
        }),
    )));

    let response = server.perform(Request::get("/")).unwrap();
    assert_eq!(response.status().as_u16(), 200);
}

#[test]
fn missing_local_data() {
    #[derive(LocalData)]
    struct Foo(String);

    let mut server = local_server(App::builder().route((
        "/",
        wrap_async(|input| {
            input
                .extract::<Local<Foo>>()
                .map(|locals| locals.with(|data| data.0.clone()))
        }),
    )));

    let response = server.perform(Request::get("/")).unwrap();
    assert_eq!(response.status().as_u16(), 500);
}

#[test]
fn either() {
    let mut server = local_server(App::builder().route((
        "/",
        "POST",
        wrap_async(|input| {
            #[derive(Debug, serde::Deserialize)]
            struct Params {
                id: u32,
                name: String,
            }
            input
                .extract::<Either<Json<Params>, Urlencoded<Params>>>()
                .map(|params| match params {
                    Either::Left(Json(params)) => params,
                    Either::Right(Urlencoded(params)) => params,
                }).map(|params| format!("{},{}", params.id, params.name))
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
