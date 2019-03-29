use {
    http::{Method, Request, StatusCode},
    tsukuyomi::{
        endpoint, extractor,
        extractor::ExtractorExt,
        path,
        test::{self, loc, TestServer},
        App,
    },
};

#[test]
fn unit_input() -> test::Result {
    let app = App::build(|mut scope| {
        scope
            .at("/")? //
            .to(endpoint::call(|| "dummy"))
    })?;
    let mut server = TestServer::new(app)?;
    let mut client = server.connect();

    client.get("/").assert(loc!(), StatusCode::OK)?;

    Ok(())
}

#[test]
fn params() -> test::Result {
    let app = App::build(|mut scope| {
        scope
            .at(path!("/:id/:name/*path"))? //
            .to(endpoint::call(|id: u32, name: String, path: String| {
                format!("{},{},{}", id, name, path)
            }))
    })?;
    let mut server = TestServer::new(app)?;
    let mut client = server.connect();

    client
        .get("/23/bob/path/to/file")
        .assert(loc!(), test::body::eq("23,bob,path/to/file"))?;

    client
        .get("/42/alice/")
        .assert(loc!(), test::body::eq("42,alice,"))?;

    Ok(())
}

#[test]
fn route_macros() -> test::Result {
    let app = App::build(|mut scope| {
        scope.at("/root")?.to(endpoint::call(|| "root"))?;

        scope
            .at(path!("/params/:id/:name"))? //
            .to(endpoint::call(|id: i32, name: String| {
                format!("params(id={}, name={})", id, name)
            }))?;

        scope
            .at(path!("/posts/:id/edit"))?
            .put()
            .extract(extractor::body::plain())
            .to(endpoint::call(|id: u32, body: String| {
                format!("posts(id={}, body={})", id, body)
            }))?;

        scope
            .at(path!("/static/*path"))?
            .to(endpoint::call(|path: String| {
                format!("static(path={})", path)
            }))
    })?;
    let mut server = TestServer::new(app)?;
    let mut client = server.connect();

    client.get("/root").assert(loc!(), test::body::eq("root"))?;

    client
        .get("/params/42/alice")
        .assert(loc!(), test::body::eq("params(id=42, name=alice)"))?;

    client
        .request(
            Request::put("/posts/1/edit")
                .header("content-type", "text/plain; charset=utf-8")
                .body("fox")?,
        )
        .assert(loc!(), test::body::eq("posts(id=1, body=fox)"))?;

    client
        .get("/static/path/to/file.txt")
        .assert(loc!(), test::body::eq("static(path=path/to/file.txt)"))?;

    Ok(())
}

#[test]
fn plain_body() -> test::Result {
    let app = App::build(|mut scope| {
        scope
            .at("/")?
            .post()
            .extract(extractor::body::plain())
            .to(endpoint::call(|body: String| body))
    })?;
    let mut server = TestServer::new(app)?;
    let mut client = server.connect();

    const BODY: &[u8] = b"The quick brown fox jumps over the lazy dog";

    client
        .request(
            Request::post("/")
                .header("content-type", "text/plain; charset=utf-8")
                .body(BODY)?,
        )
        .assert(loc!(), StatusCode::OK)?;

    // missing content-type
    client
        .request(Request::post("/").body(BODY)?)
        .assert(loc!(), StatusCode::OK)?;

    // invalid content-type
    client
        .request(
            Request::post("/")
                .header("content-type", "application/graphql")
                .body(BODY)?,
        )
        .assert(loc!(), StatusCode::BAD_REQUEST)?;

    // invalid charset
    client
        .request(
            Request::post("/")
                .header("content-type", "text/plain; charset=euc-jp")
                .body(BODY)?,
        )
        .assert(loc!(), StatusCode::BAD_REQUEST)?;

    Ok(())
}

#[test]
fn json_body() -> test::Result {
    #[derive(Debug, serde::Deserialize)]
    struct Params {
        id: u32,
        name: String,
    }

    let app = App::build(|mut scope| {
        scope
            .at("/")?
            .post()
            .extract(extractor::body::json())
            .to(endpoint::call(|params: Params| {
                format!("{},{}", params.id, params.name)
            }))
    })?;
    let mut server = TestServer::new(app)?;
    let mut client = server.connect();

    client
        .request(
            Request::post("/")
                .header("content-type", "application/json")
                .body(&br#"{"id":23, "name":"bob"}"#[..])?,
        )
        .assert(loc!(), StatusCode::OK)?
        .assert(loc!(), test::body::eq("23,bob"))?;

    // missing content-type
    client
        .request(Request::post("/").body(&br#"{"id":23, "name":"bob"}"#[..])?)
        .assert(loc!(), StatusCode::BAD_REQUEST)?;

    // invalid content-type
    client
        .request(
            Request::post("/")
                .header("content-type", "application/graphql")
                .body(&br#"{"id":23, "name":"bob"}"#[..])?,
        )
        .assert(loc!(), StatusCode::BAD_REQUEST)?;

    // invalid data
    client
        .request(
            Request::post("/")
                .header("content-type", "application/json")
                .body(&br#"THIS_IS_INVALID_JSON_DATA"#[..])?,
        )
        .assert(loc!(), StatusCode::BAD_REQUEST)?;

    Ok(())
}

#[test]
fn urlencoded_body() -> test::Result {
    #[derive(Debug, serde::Deserialize)]
    struct Params {
        id: u32,
        name: String,
    }

    let app = App::build(|mut scope| {
        scope
            .at("/")?
            .post()
            .extract(extractor::body::urlencoded())
            .to(endpoint::call(|params: Params| {
                format!("{},{}", params.id, params.name)
            }))
    })?;
    let mut server = TestServer::new(app)?;
    let mut client = server.connect();

    const BODY: &[u8] = b"id=23&name=bob";

    client
        .request(
            Request::post("/")
                .header("content-type", "application/x-www-form-urlencoded")
                .body(BODY)?,
        )
        .assert(loc!(), test::body::eq("23,bob"))?;

    // missing content-type
    client
        .request(Request::post("/").body(BODY)?)
        .assert(loc!(), StatusCode::BAD_REQUEST)?;

    // invalid content-type
    client
        .request(
            Request::post("/")
                .header("content-type", "application/graphql")
                .body(BODY)?,
        )
        .assert(loc!(), StatusCode::BAD_REQUEST)?;

    // invalid data
    client
        .request(
            Request::post("/")
                .header("content-type", "application/x-www-form-urlencoded")
                .body(&br#"THIS_IS_INVALID_FORM_DATA"#[..])?,
        )
        .assert(loc!(), StatusCode::BAD_REQUEST)?;

    Ok(())
}

#[test]
fn local_data() -> test::Result {
    use {
        futures01::Poll,
        tsukuyomi::{
            future::TryFuture,
            handler::{Handler, ModifyHandler},
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
        type Handle = InsertMyDataHandle<H::Handle>;

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

    let app = App::build(|mut scope| {
        scope
            .at("/")?
            .any()
            .with(InsertMyData::default())
            .extract(extractor::local::remove(&MyData::KEY))
            .to(endpoint::call(|x: MyData| x.0))
    })?;
    let mut server = TestServer::new(app)?;
    let mut client = server.connect();

    client.get("/").assert(loc!(), StatusCode::OK)?;

    Ok(())
}

#[test]
fn missing_local_data() -> test::Result {
    use tsukuyomi::input::localmap::local_key;

    #[derive(Clone)]
    struct MyData(String);

    impl MyData {
        local_key! {
            const KEY: Self;
        }
    }

    let app = App::build(|mut scope| {
        scope
            .at("/")?
            .any()
            .extract(extractor::local::remove(&MyData::KEY))
            .to(endpoint::call(|x: MyData| x.0))
    })?;
    let mut server = TestServer::new(app)?;
    let mut client = server.connect();

    client
        .get("/")
        .assert(loc!(), StatusCode::INTERNAL_SERVER_ERROR)?;

    Ok(())
}

#[test]
fn optional() -> test::Result {
    #[derive(Debug, serde::Deserialize)]
    struct Params {
        id: u32,
        name: String,
    }

    let extractor = extractor::body::json().optional();

    let app = App::build(|mut scope| {
        scope
            .at("/")?
            .post()
            .extract(extractor)
            .to(endpoint::call_async(|params: Option<Params>| {
                if let Some(params) = params {
                    Ok(format!("{},{}", params.id, params.name))
                } else {
                    Err(tsukuyomi::error::internal_server_error("####none####"))
                }
            }))
    })?;
    let mut server = TestServer::new(app)?;
    let mut client = server.connect();

    client
        .request(
            Request::post("/")
                .header("content-type", "application/json")
                .body(&br#"{"id":23, "name":"bob"}"#[..])?,
        )
        .assert(loc!(), StatusCode::OK)?
        .assert(loc!(), test::body::eq("23,bob"))?;

    client
        .request(
            Request::post("/")
                .header("content-type", "application/x-www-form-urlencoded")
                .body(&b"id=23&name=bob"[..])?,
        )
        .assert(loc!(), StatusCode::INTERNAL_SERVER_ERROR)?;

    Ok(())
}

#[test]
fn either_or() -> test::Result {
    #[derive(Debug, serde::Deserialize)]
    struct Params {
        id: u32,
        name: String,
    }

    let params_extractor = (extractor::method::get().and(extractor::query()))
        .or(extractor::method::post().and(extractor::body::json()))
        .or(extractor::method::post().and(extractor::body::urlencoded()));

    let app = App::build(|mut scope| {
        scope
            .at("/")?
            .route(&[Method::GET, Method::POST])
            .extract(params_extractor)
            .to(endpoint::call(|params: Params| {
                format!("{},{}", params.id, params.name)
            }))
    })?;
    let mut server = TestServer::new(app)?;
    let mut client = server.connect();

    client
        .get("/?id=23&name=bob")
        .assert(loc!(), StatusCode::OK)?
        .assert(loc!(), test::body::eq("23,bob"))?;

    client
        .request(
            Request::post("/")
                .header("content-type", "application/json")
                .body(&br#"{"id":23, "name":"bob"}"#[..])?,
        )
        .assert(loc!(), StatusCode::OK)?
        .assert(loc!(), test::body::eq("23,bob"))?;

    client
        .request(
            Request::post("/")
                .header("content-type", "application/x-www-form-urlencoded")
                .body(&b"id=23&name=bob"[..])?,
        )
        .assert(loc!(), StatusCode::OK)?
        .assert(loc!(), test::body::eq("23,bob"))?;

    client
        .request(
            Request::post("/")
                .header("content-type", "text/plain; charset=utf-8")
                .body(&b"///invalid string"[..])?,
        )
        .assert(loc!(), StatusCode::BAD_REQUEST)?;

    Ok(())
}
