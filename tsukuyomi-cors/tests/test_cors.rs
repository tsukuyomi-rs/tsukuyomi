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
        Method, Request,
    },
    tsukuyomi::{
        config::prelude::*, //
        App,
    },
    tsukuyomi_cors::CORS,
    tsukuyomi_server::test::ResponseExt,
};

#[test]
fn test_version_sync() {
    version_sync::assert_html_root_url_updated!("src/lib.rs");
}

#[test]
fn simple_request_with_default_configuration() -> tsukuyomi_server::Result<()> {
    let cors = CORS::new();

    let app = App::create(
        path!("/") //
            .to(endpoint::get() //
                .call(|| "hello"))
            .modify(cors),
    )?;
    let mut server = tsukuyomi_server::test::server(app)?;

    let response = server.perform(
        Request::get("/")
            .header(HOST, "localhost")
            .header(ORIGIN, "http://example.com"),
    )?;
    assert_eq!(response.status(), 200);
    assert_eq!(response.body().to_utf8()?, "hello");
    assert_eq!(response.header(ACCESS_CONTROL_ALLOW_ORIGIN)?, "*");

    // without origin header
    let response = server.perform(
        Request::get("/") //
            .header(HOST, "localhost"),
    )?;
    assert_eq!(response.status(), 200);
    assert_eq!(response.body().to_utf8()?, "hello");
    assert!(!response.headers().contains_key(ACCESS_CONTROL_ALLOW_ORIGIN));

    Ok(())
}

#[test]
fn simple_request_with_allow_origin() -> tsukuyomi_server::Result<()> {
    let cors = CORS::builder().allow_origin("http://example.com")?.build();

    let app = App::create(
        path!("/") //
            .to(endpoint::get() //
                .call(|| "hello"))
            .modify(cors),
    )?;
    let mut server = tsukuyomi_server::test::server(app)?;

    let response = server.perform(
        Request::get("/")
            .header(HOST, "localhost")
            .header(ORIGIN, "http://example.com"),
    )?;
    assert_eq!(response.status(), 200);
    assert_eq!(response.body().to_utf8()?, "hello");
    assert_eq!(
        response.header(ACCESS_CONTROL_ALLOW_ORIGIN)?,
        "http://example.com"
    );

    // disallowed origin
    let response = server.perform(
        Request::get("/")
            .header(HOST, "localhost")
            .header(ORIGIN, "http://example.org"),
    )?;
    assert_eq!(response.status(), 403);

    Ok(())
}

#[test]
fn simple_request_with_allow_method() -> tsukuyomi_server::Result<()> {
    let cors = CORS::builder() //
        .allow_method(Method::GET)?
        .build();

    let app = App::create(
        path!("/") //
            .to(endpoint::allow_only("GET, DELETE")? //
                .call(|| "hello"))
            .modify(cors),
    )?;
    let mut server = tsukuyomi_server::test::server(app)?;

    let response = server.perform(
        Request::get("/")
            .header(HOST, "localhost")
            .header(ORIGIN, "http://example.com"),
    )?;
    assert_eq!(response.status(), 200);
    assert_eq!(response.body().to_utf8()?, "hello");
    assert_eq!(response.header(ACCESS_CONTROL_ALLOW_ORIGIN)?, "*");

    // disallowed method
    let response = server.perform(
        Request::delete("/")
            .header(HOST, "localhost")
            .header(ORIGIN, "http://example.com"),
    )?;
    assert_eq!(response.status(), 403);

    Ok(())
}

#[test]
fn simple_request_with_allow_credentials() -> tsukuyomi_server::Result<()> {
    let cors = CORS::builder() //
        .allow_credentials(true)
        .build();

    let app = App::create(
        path!("/") //
            .to(endpoint::get() //
                .call(|| "hello"))
            .modify(cors),
    )?;
    let mut server = tsukuyomi_server::test::server(app)?;

    let response = server.perform(
        Request::get("/")
            .header(HOST, "localhost")
            .header(ORIGIN, "http://example.com")
            .header(COOKIE, "session=xxxx"),
    )?;
    assert_eq!(response.status(), 200);
    assert_eq!(response.body().to_utf8()?, "hello");
    assert_eq!(
        response.header(ACCESS_CONTROL_ALLOW_ORIGIN)?,
        "http://example.com"
    );
    assert_eq!(response.header(ACCESS_CONTROL_ALLOW_CREDENTIALS)?, "true",);

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
fn preflight_with_default_configuration() -> tsukuyomi_server::Result<()> {
    let cors = CORS::new();

    let app = App::create(chain![
        path!("*").to(cors.clone()), // OPTIONS *
        path!("/") //
            .to(endpoint::get() //
                .call(|| "hello"))
            .modify(cors)  // OPTIONS /
    ])?;
    let mut server = tsukuyomi_server::test::server(app)?;

    let response = server.perform(
        Request::options("*")
            .header(HOST, "localhost")
            .header(ORIGIN, "http://example.com")
            .header(ACCESS_CONTROL_REQUEST_METHOD, "GET"),
    )?;
    assert_eq!(response.status(), 204);
    assert_eq!(response.header(ACCESS_CONTROL_ALLOW_ORIGIN)?, "*");
    assert_methods!(
        response.header(ACCESS_CONTROL_ALLOW_METHODS)?,
        [GET, POST, OPTIONS]
    );

    Ok(())
}

#[test]
fn preflight_with_allow_origin() -> tsukuyomi_server::Result<()> {
    let cors = CORS::builder().allow_origin("http://example.com")?.build();

    let app = App::create(chain![
        path!("*").to(cors.clone()), // OPTIONS *
        path!("/") //
            .to(endpoint::get() //
                .call(|| "hello"))
            .modify(cors)
    ])?;
    let mut server = tsukuyomi_server::test::server(app)?;

    let response = server.perform(
        Request::options("*")
            .header(HOST, "localhost")
            .header(ORIGIN, "http://example.com")
            .header(ACCESS_CONTROL_REQUEST_METHOD, "GET"),
    )?;
    assert_eq!(response.status(), 204);

    let response = server.perform(
        Request::options("*")
            .header(HOST, "localhost")
            .header(ORIGIN, "http://example.org")
            .header(ACCESS_CONTROL_REQUEST_METHOD, "GET"),
    )?;
    assert_eq!(response.status(), 403);

    Ok(())
}

#[test]
fn preflight_with_allow_method() -> tsukuyomi_server::Result<()> {
    let cors = CORS::builder() //
        .allow_method(Method::GET)?
        .build();

    let app = App::create(chain![
        path!("*").to(cors.clone()), // OPTIONS *
        path!("/")
            .to(endpoint::get() //
                .call(|| "hello"))
            .modify(cors)  // OPTIONS /
    ])?;
    let mut server = tsukuyomi_server::test::server(app)?;

    let response = server.perform(
        Request::options("*")
            .header(HOST, "localhost")
            .header(ORIGIN, "http://example.com")
            .header(ACCESS_CONTROL_REQUEST_METHOD, "GET"),
    )?;
    assert_eq!(response.status(), 204);

    let response = server.perform(
        Request::options("*")
            .header(HOST, "localhost")
            .header(ORIGIN, "http://example.org")
            .header(ACCESS_CONTROL_REQUEST_METHOD, "DELETE"),
    )?;
    assert_eq!(response.status(), 403);

    Ok(())
}

#[test]
fn preflight_with_allow_headers() -> tsukuyomi_server::Result<()> {
    const X_API_KEY: &str = "x-api-key";

    let cors = CORS::builder() //
        .allow_header(X_API_KEY)?
        .build();

    let app = App::create(chain![
        path!("*").to(cors.clone()), // OPTIONS *
        path!("/") //
            .to(endpoint::get() //
                .call(|| "hello"))
            .modify(cors)
    ])?;
    let mut server = tsukuyomi_server::test::server(app)?;

    let response = server.perform(
        Request::options("*")
            .header(HOST, "localhost")
            .header(ORIGIN, "http://example.com")
            .header(ACCESS_CONTROL_REQUEST_METHOD, "GET")
            .header(ACCESS_CONTROL_REQUEST_HEADERS, X_API_KEY),
    )?;
    assert_eq!(response.status(), 204);
    assert_headers!(
        response.header(ACCESS_CONTROL_ALLOW_HEADERS)?,
        [X_API_KEY.parse().unwrap()]
    );

    let response = server.perform(
        Request::options("*")
            .header(HOST, "localhost")
            .header(ORIGIN, "http://example.org")
            .header(ACCESS_CONTROL_REQUEST_METHOD, "GET")
            .header(ACCESS_CONTROL_REQUEST_HEADERS, "authorization"),
    )?;
    assert_eq!(response.status(), 403);

    Ok(())
}

#[test]
fn preflight_max_age() -> tsukuyomi_server::Result<()> {
    const SECS_PER_DAY: i64 = 60 * 60 * 24;

    let cors = CORS::builder() //
        .max_age(std::time::Duration::from_secs(SECS_PER_DAY as u64))
        .build();

    let app = App::create(chain![
        path!("*").to(cors.clone()),
        path!("/") //
            .to(endpoint::get() //
                .call(|| "hello"))
            .modify(cors)
    ])?;
    let mut server = tsukuyomi_server::test::server(app)?;

    let response = server.perform(
        Request::options("*")
            .header(HOST, "localhost")
            .header(ORIGIN, "http://example.com")
            .header(ACCESS_CONTROL_REQUEST_METHOD, "GET"),
    )?;
    assert_eq!(response.status(), 204);
    assert_eq!(
        response.header(ACCESS_CONTROL_MAX_AGE)?,
        SECS_PER_DAY.to_string().as_str()
    );

    Ok(())
}

#[test]
fn as_route_modifier() -> tsukuyomi_server::Result<()> {
    let cors = CORS::new();

    let app = App::create(chain![
        path!("/cors") //
            .to(endpoint::get() //
                .call(|| "cors"))
            .modify(cors.clone()),
        path!("/nocors") //
            .to(endpoint::get().call(|| "nocors")),
        path!("*").to(cors),
    ])?;
    let mut server = tsukuyomi_server::test::server(app)?;

    let response = server.perform(
        Request::get("/cors") //
            .header(ORIGIN, "http://example.com"),
    )?;
    assert_eq!(response.status(), 200);
    assert_eq!(response.header(ACCESS_CONTROL_ALLOW_ORIGIN)?, "*");

    let response = server.perform(
        Request::options("/cors") //
            .header(ORIGIN, "http://example.com")
            .header(ACCESS_CONTROL_REQUEST_METHOD, "GET"),
    )?;
    assert_eq!(response.status(), 204);
    assert_eq!(response.header(ACCESS_CONTROL_ALLOW_ORIGIN)?, "*");

    let response = server.perform(
        Request::options("*")
            .header(ORIGIN, "http://example.com")
            .header(ACCESS_CONTROL_REQUEST_METHOD, "GET"),
    )?;
    assert_eq!(response.status(), 204);
    assert_eq!(response.header(ACCESS_CONTROL_ALLOW_ORIGIN)?, "*");

    let response = server.perform(
        Request::get("/nocors") //
            .header(ORIGIN, "http://example.com"),
    )?;
    assert!(!response.headers().contains_key(ACCESS_CONTROL_ALLOW_ORIGIN));

    Ok(())
}

#[test]
fn as_scope_modifier() -> tsukuyomi_server::Result<()> {
    let cors = CORS::new();

    let app = App::create(chain![
        path!("/cors") //
            .to(endpoint::get().call(|| "cors"))
            .modify(cors),
        path!("/nocors") //
            .to(endpoint::get() //
                .call(|| "nocors")),
    ])?;
    let mut server = tsukuyomi_server::test::server(app)?;

    let response = server.perform(
        Request::get("/cors") //
            .header(ORIGIN, "http://example.com"),
    )?;
    assert_eq!(response.status(), 200);
    assert_eq!(response.header(ACCESS_CONTROL_ALLOW_ORIGIN)?, "*");

    let response = server.perform(
        Request::options("/cors") //
            .header(ORIGIN, "http://example.com")
            .header(ACCESS_CONTROL_REQUEST_METHOD, "GET"),
    )?;
    assert_eq!(response.status(), 204);
    assert_eq!(response.header(ACCESS_CONTROL_ALLOW_ORIGIN)?, "*");

    let response = server.perform(
        Request::get("/nocors") //
            .header(ORIGIN, "http://example.com"),
    )?;
    assert!(!response.headers().contains_key(ACCESS_CONTROL_ALLOW_ORIGIN));

    Ok(())
}
