use {
    http::Request,
    tsukuyomi::{
        app::config::prelude::*, //
        chain,
        endpoint,
        extractor,
        extractor::{Extractor, ExtractorExt},
        server::Server,
        App,
    },
};

#[test]
fn unit_input() -> tsukuyomi::test::Result<()> {
    let mut server = App::configure({
        route() //
            .to(endpoint::any().reply(|| "dummy"))
    })
    .map(Server::new)?
    .into_test_server()?;
    let response = server.perform("/")?;
    assert_eq!(response.status(), 200);
    Ok(())
}

#[test]
fn params() -> tsukuyomi::test::Result<()> {
    let mut server = App::configure({
        route()
            .param("id")?
            .param("name")?
            .catch_all("path")? //
            .to(endpoint::any()
                .reply(|id: u32, name: String, path: String| format!("{},{},{}", id, name, path)))
    })
    .map(Server::new)?
    .into_test_server()?;

    let response = server.perform("/23/bob/path/to/file")?;
    assert_eq!(response.body().to_utf8()?, "23,bob,path/to/file");

    let response = server.perform("/42/alice/")?;
    assert_eq!(response.body().to_utf8()?, "42,alice,");

    Ok(())
}

#[test]
fn route_macros() -> tsukuyomi::app::Result<()> {
    App::configure(chain![
        route()
            .segment("root")?
            .to(endpoint::any().reply(|| "root")),
        route()
            .segment("params")?
            .param("id")?
            .param("name")?
            .to(endpoint::any().reply(|id: i32, name: String| {
                drop((id, name));
                "dummy"
            })),
        route().segment("posts")?.param("id")?.segment("edit")?.to({
            endpoint::put()
                .extract(extractor::body::plain::<String>())
                .reply(|id: u32, body: String| {
                    drop((id, body));
                    "dummy"
                })
        }),
        route()
            .segment("static")?
            .catch_all("path")?
            .to(endpoint::any().reply(|path: String| {
                drop(path);
                "dummy"
            })),
    ])
    .map(drop)
}

#[test]
fn plain_body() -> tsukuyomi::test::Result<()> {
    let mut server = App::configure(
        route().to(endpoint::post()
            .extract(extractor::body::plain())
            .reply(|body: String| body)),
    )
    .map(Server::new)?
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

    let mut server = App::configure(
        route().to(endpoint::post()
            .extract(extractor::body::json())
            .reply(|params: Params| format!("{},{}", params.id, params.name))),
    )
    .map(Server::new)?
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

    let mut server = App::configure(
        route().to(endpoint::post()
            .extract(extractor::body::urlencoded())
            .reply(|params: Params| format!("{},{}", params.id, params.name))),
    )
    .map(Server::new)?
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
        handler::{AllowedMethods, Handler, ModifyHandler},
        input::localmap::local_key,
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
        type Future = H::Future;

        fn allowed_methods(&self) -> Option<&AllowedMethods> {
            self.0.allowed_methods()
        }

        fn call(
            &self,
            input: &mut tsukuyomi::Input<'_>,
        ) -> tsukuyomi::future::MaybeFuture<Self::Future> {
            input.locals.insert(&MyData::KEY, MyData("dummy".into()));
            self.0.call(input)
        }
    }

    let mut server = App::configure(with_modifier(InsertMyData::default(), {
        route().to(endpoint::any()
            .extract(extractor::local::remove(&MyData::KEY))
            .reply(|x: MyData| x.0))
    }))
    .map(Server::new)?
    .into_test_server()?;

    let response = server.perform("/")?;
    assert_eq!(response.status(), 200);

    Ok(())
}

#[test]
fn missing_local_data() -> tsukuyomi::test::Result<()> {
    use tsukuyomi::input::localmap::local_key;

    #[derive(Clone)]
    struct MyData(String);

    impl MyData {
        local_key! {
            const KEY: Self;
        }
    }

    let mut server = App::configure({
        route().to(endpoint::any()
            .extract(extractor::local::remove(&MyData::KEY))
            .reply(|x: MyData| x.0))
    })
    .map(Server::new)?
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

    let extractor = ExtractorExt::new(extractor::body::json()).optional();

    let mut server = App::configure(
        route() //
            .to({
                endpoint::post() //
                    .extract(extractor)
                    .reply(|params: Option<Params>| {
                        if let Some(params) = params {
                            Ok(format!("{},{}", params.id, params.name))
                        } else {
                            Err(tsukuyomi::error::internal_server_error("####none####"))
                        }
                    })
            }),
    )
    .map(Server::new)?
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

    let params_extractor =
        ExtractorExt::new(extractor::method::get().chain(extractor::query::query()))
            .either_or(extractor::method::post().chain(extractor::body::json()))
            .either_or(extractor::method::post().chain(extractor::body::urlencoded()));

    let mut server = App::configure(
        route() //
            .to({
                endpoint::allow_only("GET, POST")?
                    .extract(params_extractor)
                    .reply(|params: Params| format!("{},{}", params.id, params.name))
            }),
    )
    .map(Server::new)?
    .into_test_server()?;

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
