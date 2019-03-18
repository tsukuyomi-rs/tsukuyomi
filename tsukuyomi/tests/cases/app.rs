use {
    http::{
        header::{self, CONTENT_LENGTH, CONTENT_TYPE},
        Request, StatusCode,
    },
    tsukuyomi::{
        config::prelude::*, //
        test::{self, loc, TestServer},
        App,
    },
};

#[test]
fn empty_routes() -> test::Result {
    let mut server = TestServer::new(App::create(())?)?;
    let mut client = server.connect();

    client
        .get("/") //
        .assert(loc!(), StatusCode::NOT_FOUND)?;

    Ok(())
}

#[test]
fn single_route() -> test::Result {
    let mut server = TestServer::new(App::create(
        path!("/hello") //
            .to(endpoint::call(|| "Tsukuyomi")),
    )?)?;
    let mut client = server.connect();

    client
        .get("/hello")
        .assert(loc!(), StatusCode::OK)?
        .assert(
            loc!(),
            test::header::eq(CONTENT_TYPE, "text/plain; charset=utf-8"),
        )?
        .assert(loc!(), test::header::eq(CONTENT_LENGTH, "9"))?
        .assert(loc!(), test::body::eq("Tsukuyomi"))?;

    Ok(())
}

#[test]
fn post_body() -> test::Result {
    let mut server = TestServer::new(App::create(
        path!("/hello") //
            .to(endpoint::post()
                .extract(tsukuyomi::extractor::body::plain())
                .call(|body: String| body)),
    )?)?;
    let mut client = server.connect();

    client
        .request(
            Request::post("/hello") //
                .body("Hello, Tsukuyomi.")?,
        )
        .assert(loc!(), StatusCode::OK)?
        .assert(
            loc!(),
            test::header::eq(CONTENT_TYPE, "text/plain; charset=utf-8"),
        )?
        .assert(loc!(), test::header::eq(CONTENT_LENGTH, "17"))?
        .assert(loc!(), test::body::eq("Hello, Tsukuyomi."))?;

    Ok(())
}

// #[test]
// fn cookies() -> izanami::Result<()> {
//     use cookie::Cookie;
//     use time::Duration;

//     let expires_in = time::now() + Duration::days(7);

//     let app = App::create(chain![
//         path!("/login") //
//             .to(endpoint::any()
//                 .extract(extractor::ready(move |input| {
//                     input.cookies.jar()?.add(
//                         Cookie::build("session", "dummy_session_id")
//                             .domain("www.example.com")
//                             .expires(expires_in)
//                             .finish(),
//                     );
//                     Ok::<_, tsukuyomi::error::Error>(())
//                 }))
//                 .call(|| "Logged in")),
//         path!("/logout") //
//             .to(endpoint::any()
//                 .extract(extractor::ready(|input| {
//                     input.cookies.jar()?.remove(Cookie::named("session"));
//                     Ok::<_, tsukuyomi::error::Error>(())
//                 }))
//                 .call(|| "Logged out")),
//     ])?;
//     let mut server = izanami::test::server(app)?;

//     let response = server.perform("/login")?;

//     let cookie_str = response.header(header::SET_COOKIE)?.to_str()?;
//     let cookie = Cookie::parse_encoded(cookie_str)?;
//     assert_eq!(cookie.name(), "session");
//     assert_eq!(cookie.domain(), Some("www.example.com"));
//     assert_eq!(
//         cookie.expires().map(|tm| tm.to_timespec().sec),
//         Some(expires_in.to_timespec().sec)
//     );

//     let response = server.perform(Request::get("/logout").header(header::COOKIE, cookie_str))?;

//     let cookie_str = response.header(header::SET_COOKIE)?.to_str()?;
//     let cookie = Cookie::parse_encoded(cookie_str)?;
//     assert_eq!(cookie.name(), "session");
//     assert_eq!(cookie.value(), "");
//     assert_eq!(cookie.max_age(), Some(Duration::zero()));
//     assert!(cookie.expires().map_or(false, |tm| tm < time::now()));

//     let response = server.perform("/logout")?;
//     assert!(!response.headers().contains_key(header::SET_COOKIE));

//     Ok(())
// }

#[test]
fn default_options() -> test::Result {
    let mut server = TestServer::new(App::create(
        path!("/path")
            .to(endpoint::allow_only("GET, POST")?.call(|| "reply"))
            .modify(tsukuyomi::modifiers::default_options()),
    )?)?;
    let mut client = server.connect();

    client
        .get("/path")
        .assert(loc!(), StatusCode::OK)?
        .assert(loc!(), test::body::eq("reply"))?;

    client
        .request(Request::options("/path").body("")?)
        .assert(loc!(), StatusCode::NO_CONTENT)?
        .assert(
            loc!(),
            test::header::eq(header::ALLOW, "GET, POST, OPTIONS"),
        )?;

    Ok(())
}

#[test]
fn map_output() -> test::Result {
    let mut server = TestServer::new(App::create(
        path!("/")
            .to(endpoint::reply(42))
            .modify(tsukuyomi::modifiers::map_output(|num: u32| num.to_string())),
    )?)?;
    let mut client = server.connect();

    client
        .get("/")
        .assert(loc!(), StatusCode::OK)?
        .assert(loc!(), test::body::eq("42"))?;

    Ok(())
}

#[test]
fn scoped_fallback() -> test::Result {
    use std::sync::{Arc, Mutex};

    let marker = Arc::new(Mutex::new(vec![]));

    let mut server = TestServer::new(App::create(chain![
        path!("*") //
            .to(endpoint::call({
                let marker = marker.clone();
                move || {
                    marker.lock().unwrap().push("F1");
                    "f1"
                }
            })),
        mount("/api/v1/").with(chain![
            path!("*") //
                .to(endpoint::call({
                    let marker = marker.clone();
                    move || {
                        marker.lock().unwrap().push("F2");
                        "f2"
                    }
                })),
            path!("/posts") //
                .to(endpoint::post().reply("posts")),
            mount("/events").with(
                path!("/new") //
                    .to(endpoint::post().reply("new_event")),
            ),
        ]),
    ])?)?;
    let mut client = server.connect();

    client.get("/");
    assert_eq!(&**marker.lock().unwrap(), &*vec!["F1"]);

    marker.lock().unwrap().clear();
    client.get("/api/v1/p");
    assert_eq!(&**marker.lock().unwrap(), &*vec!["F2"]);

    marker.lock().unwrap().clear();
    client.get("/api/v1/posts");
    assert!(marker.lock().unwrap().is_empty());

    marker.lock().unwrap().clear();
    client.get("/api/v1/events/new");
    assert!(marker.lock().unwrap().is_empty());

    Ok(())
}
