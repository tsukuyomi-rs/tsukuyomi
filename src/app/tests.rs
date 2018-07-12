use super::*;

use handler::Handler;

#[test]
fn empty() {
    let app = App::builder().finish().unwrap();
    assert!(app.global.recognize("/", &Method::GET).is_err());
}

#[test]
fn root_single_method() {
    let app = App::builder()
        .mount("/", |m| {
            m.get("/").handle(Handler::new_ready(|_| "a"));
        })
        .finish()
        .unwrap();

    assert_matches!(app.global.recognize("/", &Method::GET), Ok(Recognize::Matched(0, _)));

    assert!(app.global.recognize("/path/to", &Method::GET).is_err());
    assert!(app.global.recognize("/", &Method::POST).is_err());
}

#[test]
fn root_multiple_method() {
    let app = App::builder()
        .mount("/", |m| {
            m.get("/").handle(Handler::new_ready(|_| "a"));
            m.post("/").handle(Handler::new_ready(|_| "b"));
        })
        .finish()
        .unwrap();

    assert_matches!(app.global.recognize("/", &Method::GET), Ok(Recognize::Matched(0, _)));
    assert_matches!(app.global.recognize("/", &Method::POST), Ok(Recognize::Matched(1, _)));

    assert!(app.global.recognize("/", &Method::PUT).is_err());
}

#[test]
fn root_fallback_head() {
    let app = App::builder()
        .mount("/", |m| {
            m.get("/").handle(Handler::new_ready(|_| "a"));
        })
        .finish()
        .unwrap();

    assert_matches!(app.global.recognize("/", &Method::HEAD), Ok(Recognize::Matched(0, _)));
}

#[test]
fn root_fallback_head_disabled() {
    let app = App::builder()
        .mount("/", |m| {
            m.get("/").handle(Handler::new_ready(|_| "a"));
        })
        .fallback_head(false)
        .finish()
        .unwrap();

    assert!(app.global.recognize("/", &Method::HEAD).is_err());
}

#[test]
fn fallback_options() {
    let app = App::builder()
        .mount("/path/to", |m| {
            m.get("/foo").handle(Handler::new_ready(|_| "a"));
            m.post("/foo").handle(Handler::new_ready(|_| "b"));
        })
        .fallback_options(true)
        .finish()
        .unwrap();

    // FIXME:
    assert_matches!(
        app.global.recognize("/path/to/foo", &Method::OPTIONS),
        Ok(Recognize::Options(_))
    );
}

#[test]
fn fallback_options_disabled() {
    let app = App::builder()
        .mount("/path/to", |m| {
            m.get("/foo").handle(Handler::new_ready(|_| "a"));
            m.post("/foo").handle(Handler::new_ready(|_| "b"));
        })
        .fallback_options(false)
        .finish()
        .unwrap();

    assert!(app.global.recognize("/path/to/foo", &Method::OPTIONS).is_err());
}

#[test]
fn mount() {
    let app = App::builder()
        .mount("/", |m| {
            m.get("/foo").handle(Handler::new_ready(|_| "a")); // /foo
            m.get("/bar").handle(Handler::new_ready(|_| "b")); // /bar
        })
        .mount("/baz", |m| {
            m.get("/").handle(Handler::new_ready(|_| "c")); // /baz

            m.mount("/", |m| {
                m.get("/").handle(Handler::new_ready(|_| "d")); // /baz
                m.get("/foobar").handle(Handler::new_ready(|_| "e")); // /baz/foobar
            });
        })
        .finish()
        .unwrap();

    assert_matches!(app.global.recognize("/foo", &Method::GET), Ok(Recognize::Matched(0, _)));
    assert_matches!(app.global.recognize("/bar", &Method::GET), Ok(Recognize::Matched(1, _)));
    assert_matches!(app.global.recognize("/baz", &Method::GET), Ok(Recognize::Matched(3, _)));
    assert_matches!(
        app.global.recognize("/baz/foobar", &Method::GET),
        Ok(Recognize::Matched(4, _))
    );

    assert!(app.global.recognize("/baz/", &Method::GET).is_err());
}
