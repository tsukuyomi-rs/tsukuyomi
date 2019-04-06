use {
    http::{
        header::{
            ACCESS_CONTROL_ALLOW_CREDENTIALS, //
            ACCESS_CONTROL_ALLOW_HEADERS,
            ACCESS_CONTROL_ALLOW_METHODS,
            ACCESS_CONTROL_ALLOW_ORIGIN,
            ACCESS_CONTROL_MAX_AGE,
            ACCESS_CONTROL_REQUEST_HEADERS,
            ACCESS_CONTROL_REQUEST_METHOD,
            COOKIE,
            HOST,
            ORIGIN,
        },
        Method, Request, StatusCode,
    },
    tsukuyomi::{
        endpoint,
        test::{self, loc, TestServer},
        App,
    },
    tsukuyomi_cors::CORS,
};

#[test]
fn test_version_sync() {
    version_sync::assert_html_root_url_updated!("src/lib.rs");
}

#[test]
fn simple_request_with_default_configuration() -> test::Result {
    let cors = CORS::new();

    let app = App::builder()
        .root(|mut s| {
            s.with(cors)
                .done(|mut s| s.at("/")?.get().to(endpoint::call(|| "hello")))
        })?
        .build()?;
    let mut server = TestServer::new(app)?;
    let mut client = server.connect();

    client
        .request(
            Request::get("/")
                .header(HOST, "localhost")
                .header(ORIGIN, "http://example.com")
                .body("")?,
        )
        .assert(loc!(), StatusCode::OK)?
        .assert(loc!(), test::header::eq(ACCESS_CONTROL_ALLOW_ORIGIN, "*"))?
        .assert(loc!(), test::body::eq("hello"))?;

    // without origin header
    client
        .request(
            Request::get("/") //
                .header(HOST, "localhost")
                .body("")?,
        )
        .assert(loc!(), StatusCode::OK)?
        .assert(
            loc!(),
            test::header::not_exists(ACCESS_CONTROL_ALLOW_ORIGIN),
        )?
        .assert(loc!(), test::body::eq("hello"))?;

    Ok(())
}

#[test]
fn simple_request_with_allow_origin() -> test::Result {
    let cors = CORS::builder().allow_origin("http://example.com")?.build();

    let app = App::builder()
        .root(|mut s| {
            s.with(cors).done(|mut s| {
                s.at("/")? //
                    .get()
                    .to(endpoint::call(|| "hello"))
            })
        })?
        .build()?;
    let mut server = TestServer::new(app)?;
    let mut client = server.connect();

    client
        .request(
            Request::get("/")
                .header(HOST, "localhost")
                .header(ORIGIN, "http://example.com")
                .body("")?,
        )
        .assert(loc!(), StatusCode::OK)?
        .assert(
            loc!(),
            test::header::eq(ACCESS_CONTROL_ALLOW_ORIGIN, "http://example.com"),
        )?
        .assert(loc!(), test::body::eq("hello"))?;

    // disallowed origin
    client
        .request(
            Request::get("/")
                .header(HOST, "localhost")
                .header(ORIGIN, "http://example.org")
                .body("")?,
        )
        .assert(loc!(), StatusCode::FORBIDDEN)?;

    Ok(())
}

#[test]
fn simple_request_with_allow_method() -> test::Result {
    let cors = CORS::builder() //
        .allow_method(Method::GET)?
        .build();

    let app = App::builder()
        .root(|mut s| {
            s.with(cors).done(|mut s| {
                s.at("/")?
                    .route(&[Method::GET, Method::DELETE])
                    .to(endpoint::call(|| "hello"))
            })
        })?
        .build()?;
    let mut server = TestServer::new(app)?;
    let mut client = server.connect();

    client
        .request(
            Request::get("/")
                .header(HOST, "localhost")
                .header(ORIGIN, "http://example.com")
                .body("")?,
        )
        .assert(loc!(), StatusCode::OK)?
        .assert(loc!(), test::header::eq(ACCESS_CONTROL_ALLOW_ORIGIN, "*"))?
        .assert(loc!(), test::body::eq("hello"))?;

    // disallowed method
    client
        .request(
            Request::delete("/")
                .header(HOST, "localhost")
                .header(ORIGIN, "http://example.com")
                .body("")?,
        )
        .assert(loc!(), StatusCode::FORBIDDEN)?;

    Ok(())
}

#[test]
fn simple_request_with_allow_credentials() -> test::Result {
    let cors = CORS::builder() //
        .allow_credentials(true)
        .build();

    let app = App::builder()
        .root(|mut s| {
            s.with(cors).done(|mut s| {
                s.at("/")? //
                    .get()
                    .to(endpoint::call(|| "hello"))
            })
        })?
        .build()?;
    let mut server = TestServer::new(app)?;
    let mut client = server.connect();

    client
        .request(
            Request::get("/")
                .header(HOST, "localhost")
                .header(ORIGIN, "http://example.com")
                .header(COOKIE, "session=xxxx")
                .body("")?,
        )
        .assert(loc!(), StatusCode::OK)?
        .assert(
            loc!(),
            test::header::eq(ACCESS_CONTROL_ALLOW_ORIGIN, "http://example.com"),
        )?
        .assert(
            loc!(),
            test::header::eq(ACCESS_CONTROL_ALLOW_CREDENTIALS, "true"),
        )?
        .assert(loc!(), test::body::eq("hello"))?;

    Ok(())
}

macro_rules! assert_methods {
    ($h:expr, [$($METHOD:ident),*]) => {{
        let h_str = $h.to_str()?;
        let methods: std::collections::HashSet<http::Method> = h_str
            .split(',')
            .map(|s| s.trim().parse())
            .collect::<Result<_, _>>()?;
        let expected = vec![$(http::Method::$METHOD),*].into_iter().collect();
        assert_eq!(methods, expected);
    }};
}

macro_rules! assert_headers {
    ($h:expr, [$($name:expr),*]) => {{
        let h_str = $h.to_str()?;
        let headers: std::collections::HashSet<http::header::HeaderName> = h_str
            .split(',')
            .map(|s| s.trim().parse())
            .collect::<Result<_, _>>()?;
        let expected = vec![$($name),*].into_iter().collect();
        assert_eq!(headers, expected);
    }};
}

#[test]
fn preflight_with_default_configuration() -> test::Result {
    let cors = CORS::new();

    let app = App::builder()
        .root(|mut s| {
            s.fallback(cors.clone())?;
            s.with(cors).done(|mut s| {
                s.at("/")? //
                    .get()
                    .to(endpoint::call(|| "hello"))
            })
        })?
        .build()?;
    let mut server = TestServer::new(app)?;
    let mut client = server.connect();

    let response = client
        .request(
            Request::options("*")
                .header(HOST, "localhost")
                .header(ORIGIN, "http://example.com")
                .header(ACCESS_CONTROL_REQUEST_METHOD, "GET")
                .body("")?,
        )
        .assert(loc!(), StatusCode::NO_CONTENT)?
        .assert(loc!(), test::header::eq(ACCESS_CONTROL_ALLOW_ORIGIN, "*"))?;
    assert_methods!(
        response
            .headers()
            .get(ACCESS_CONTROL_ALLOW_METHODS)
            .unwrap(),
        [GET, POST, OPTIONS]
    );

    Ok(())
}

#[test]
fn preflight_with_allow_origin() -> test::Result {
    let cors = CORS::builder().allow_origin("http://example.com")?.build();

    let app = App::builder()
        .root(|mut s| {
            s.fallback(cors.clone())?;
            s.with(cors).done(|mut s| {
                s.at("/")? //
                    .get()
                    .to(endpoint::call(|| "hello"))
            })
        })?
        .build()?;
    let mut server = TestServer::new(app)?;
    let mut client = server.connect();

    client
        .request(
            Request::options("*")
                .header(HOST, "localhost")
                .header(ORIGIN, "http://example.com")
                .header(ACCESS_CONTROL_REQUEST_METHOD, "GET")
                .body("")?,
        )
        .assert(loc!(), StatusCode::NO_CONTENT)?;

    client
        .request(
            Request::options("*")
                .header(HOST, "localhost")
                .header(ORIGIN, "http://example.org")
                .header(ACCESS_CONTROL_REQUEST_METHOD, "GET")
                .body("")?,
        )
        .assert(loc!(), StatusCode::FORBIDDEN)?;

    Ok(())
}

#[test]
fn preflight_with_allow_method() -> test::Result {
    let cors = CORS::builder() //
        .allow_method(Method::GET)?
        .build();

    let app = App::builder()
        .root(|mut s| {
            s.fallback(cors.clone())?;
            s.with(cors).done(|mut s| {
                s.at("/")? //
                    .get()
                    .to(endpoint::call(|| "hello"))
            })
        })?
        .build()?;
    let mut server = TestServer::new(app)?;
    let mut client = server.connect();

    client
        .request(
            Request::options("*")
                .header(HOST, "localhost")
                .header(ORIGIN, "http://example.com")
                .header(ACCESS_CONTROL_REQUEST_METHOD, "GET")
                .body("")?,
        )
        .assert(loc!(), StatusCode::NO_CONTENT)?;

    client
        .request(
            Request::options("*")
                .header(HOST, "localhost")
                .header(ORIGIN, "http://example.org")
                .header(ACCESS_CONTROL_REQUEST_METHOD, "DELETE")
                .body("")?,
        )
        .assert(loc!(), StatusCode::FORBIDDEN)?;

    Ok(())
}

#[test]
fn preflight_with_allow_headers() -> test::Result {
    const X_API_KEY: &str = "x-api-key";

    let cors = CORS::builder() //
        .allow_header(X_API_KEY)?
        .build();

    let app = App::builder()
        .root(|mut s| {
            s.fallback(cors.clone())?;
            s.with(cors).done(|mut s| {
                s.at("/")? //
                    .get()
                    .to(endpoint::call(|| "hello"))
            })
        })?
        .build()?;
    let mut server = TestServer::new(app)?;
    let mut client = server.connect();

    let response = client
        .request(
            Request::options("*")
                .header(HOST, "localhost")
                .header(ORIGIN, "http://example.com")
                .header(ACCESS_CONTROL_REQUEST_METHOD, "GET")
                .header(ACCESS_CONTROL_REQUEST_HEADERS, X_API_KEY)
                .body("")?,
        )
        .assert(loc!(), StatusCode::NO_CONTENT)?;
    assert_headers!(
        response
            .headers()
            .get(ACCESS_CONTROL_ALLOW_HEADERS)
            .unwrap(),
        [X_API_KEY.parse().unwrap()]
    );
    drop(response);

    client
        .request(
            Request::options("*")
                .header(HOST, "localhost")
                .header(ORIGIN, "http://example.org")
                .header(ACCESS_CONTROL_REQUEST_METHOD, "GET")
                .header(ACCESS_CONTROL_REQUEST_HEADERS, "authorization")
                .body("")?,
        )
        .assert(loc!(), StatusCode::FORBIDDEN)?;

    Ok(())
}

#[test]
fn preflight_max_age() -> test::Result {
    const SECS_PER_DAY: i64 = 60 * 60 * 24;

    let cors = CORS::builder() //
        .max_age(std::time::Duration::from_secs(SECS_PER_DAY as u64))
        .build();

    let app = App::builder()
        .root(|mut s| {
            s.fallback(cors.clone())?;
            s.with(cors).done(|mut s| {
                s.at("/")? //
                    .get()
                    .to(endpoint::call(|| "hello"))
            })
        })?
        .build()?;
    let mut server = TestServer::new(app)?;
    let mut client = server.connect();

    client
        .request(
            Request::options("*")
                .header(HOST, "localhost")
                .header(ORIGIN, "http://example.com")
                .header(ACCESS_CONTROL_REQUEST_METHOD, "GET")
                .body("")?,
        )
        .assert(loc!(), StatusCode::NO_CONTENT)?
        .assert(
            loc!(),
            test::header::eq(ACCESS_CONTROL_MAX_AGE, SECS_PER_DAY.to_string()),
        )?;

    Ok(())
}

#[test]
fn as_route_modifier() -> test::Result {
    let cors = CORS::new();

    let app = App::builder()
        .root(|mut s| {
            s.at("/cors")?.with(&cors).to(endpoint::call(|| "cors"))?;

            s.at("/nocors")? //
                .get()
                .to(endpoint::call(|| "nocors"))
        })?
        .build()?;
    let mut server = TestServer::new(app)?;
    let mut client = server.connect();

    client
        .request(
            Request::get("/cors") //
                .header(ORIGIN, "http://example.com")
                .body("")?,
        )
        .assert(loc!(), StatusCode::OK)?
        .assert(loc!(), test::header::eq(ACCESS_CONTROL_ALLOW_ORIGIN, "*"))?;

    client
        .request(
            Request::options("/cors") //
                .header(ORIGIN, "http://example.com")
                .header(ACCESS_CONTROL_REQUEST_METHOD, "GET")
                .body("")?,
        )
        .assert(loc!(), StatusCode::NO_CONTENT)?
        .assert(loc!(), test::header::eq(ACCESS_CONTROL_ALLOW_ORIGIN, "*"))?;

    client
        .request(
            Request::get("/nocors") //
                .header(ORIGIN, "http://example.com")
                .body("")?,
        )
        .assert(
            loc!(),
            test::header::not_exists(ACCESS_CONTROL_ALLOW_ORIGIN),
        )?;

    Ok(())
}

#[test]
fn as_scope_modifier() -> test::Result {
    let cors = CORS::new();

    let app = App::builder()
        .root(|mut s| {
            s.mount("/cors")?.with(&cors).done(|mut s| {
                s.at("/resource")? //
                    .to(endpoint::call(|| "cors"))
            })?;

            s.at("/nocors")? //
                .get()
                .to(endpoint::call(|| "nocors"))
        })?
        .build()?;
    let mut server = TestServer::new(app)?;
    let mut client = server.connect();

    client
        .request(
            Request::get("/cors/resource") //
                .header(ORIGIN, "http://example.com")
                .body("")?,
        )
        .assert(loc!(), StatusCode::OK)?
        .assert(loc!(), test::header::eq(ACCESS_CONTROL_ALLOW_ORIGIN, "*"))?;

    client
        .request(
            Request::options("/cors/resource") //
                .header(ORIGIN, "http://example.com")
                .header(ACCESS_CONTROL_REQUEST_METHOD, "GET")
                .body("")?,
        )
        .assert(loc!(), StatusCode::NO_CONTENT)?
        .assert(loc!(), test::header::eq(ACCESS_CONTROL_ALLOW_ORIGIN, "*"))?;

    client
        .request(
            Request::get("/nocors") //
                .header(ORIGIN, "http://example.com")
                .body("")?,
        )
        .assert(
            loc!(),
            test::header::not_exists(ACCESS_CONTROL_ALLOW_ORIGIN),
        )?;

    Ok(())
}
