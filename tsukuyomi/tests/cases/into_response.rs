use {
    http::{header, StatusCode},
    tsukuyomi::{
        config::prelude::*, //
        test::{self, loc, TestServer},
        App,
    },
};

#[test]
fn test_into_response_preset() -> test::Result {
    use {
        http::{Request, Response},
        std::fmt,
        tsukuyomi::output::preset::Preset,
    };

    struct Display;

    impl<T> Preset<T> for Display
    where
        T: std::fmt::Display,
    {
        fn into_response(
            this: T,
            _: &Request<()>,
        ) -> tsukuyomi::Result<tsukuyomi::output::Response> {
            Ok(Response::builder()
                .header("content-type", "text/plain; charset=utf-8")
                .body(this.to_string().into())
                .unwrap())
        }
    }

    #[derive(tsukuyomi::output::IntoResponse)]
    #[response(preset = "Display")]
    struct Foo(String);

    impl fmt::Display for Foo {
        fn fmt(&self, f: &mut fmt::Formatter) -> fmt::Result {
            self.0.fmt(f)
        }
    }

    #[derive(tsukuyomi::output::IntoResponse)]
    #[response(preset = "Display", bound = "T: fmt::Display")]
    struct Bar<T>(T);

    impl<T: fmt::Display> fmt::Display for Bar<T> {
        fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
            fmt::Display::fmt(&self.0, f)
        }
    }

    let app = App::create(chain! {
        path!("/foo") //
            .to(endpoint::call(|| Foo("Foo".into()))),
        path!("/bar") //
            .to(endpoint::call(|| Bar("Bar")))
    })?;

    let mut server = TestServer::new(app)?;
    let mut client = server.connect();

    client
        .get("/foo")
        .assert(loc!(), StatusCode::OK)?
        .assert(
            loc!(),
            test::header::eq(header::CONTENT_TYPE, "text/plain; charset=utf-8"),
        )?
        .assert(loc!(), test::body::eq("Foo"))?;

    client.get("/bar").assert(loc!(), test::body::eq("Bar"))?;

    Ok(())
}
