use {
    http::{
        header::{CONTENT_LENGTH, CONTENT_TYPE},
        Request, StatusCode,
    },
    tsukuyomi::{
        endpoint::builder as endpoint,
        test::{self, loc, TestServer},
        App,
    },
};

#[test]
fn empty_routes() -> test::Result {
    let app = App::build(|_| Ok(()))?;
    let mut server = TestServer::new(app)?;
    let mut client = server.connect();

    client
        .get("/") //
        .assert(loc!(), StatusCode::NOT_FOUND)?;

    Ok(())
}

#[test]
fn single_route() -> test::Result {
    let app = App::build(|s| {
        s.at("/hello", (), {
            endpoint::call(|| "Tsukuyomi") //
        })
    })?;
    let mut server = TestServer::new(app)?;
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
    let app = App::build(|s| {
        s.at("/hello", (), {
            endpoint::post()
                .extract(tsukuyomi::extractor::body::plain())
                .call(|body: String| body)
        })
    })?;
    let mut server = TestServer::new(app)?;
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
fn scoped_fallback() -> test::Result {
    use std::sync::{Arc, Mutex};

    let marker = Arc::new(Mutex::new(vec![]));

    let app = App::build(|s| {
        s.default(
            (),
            endpoint::call({
                let marker = marker.clone();
                move || {
                    marker.lock().unwrap().push("F1");
                    "f1"
                }
            }),
        )?;

        s.nest("/api/v1/", (), |s| {
            s.default(
                (),
                endpoint::call({
                    let marker = marker.clone();
                    move || {
                        marker.lock().unwrap().push("F2");
                        "f2"
                    }
                }),
            )?;
            s.at("/posts", (), {
                endpoint::post() //
                    .reply("posts")
            })?;
            s.nest("/events", (), |s| {
                s.at("/new", (), {
                    endpoint::post() //
                        .reply("new_event")
                })
            })
        })
    })?;
    let mut server = TestServer::new(app)?;
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
