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
        std::fmt,
        tsukuyomi::{
            future::{Poll, TryFuture},
            input::Input,
            output::{preset::Preset, Response},
            upgrade::NeverUpgrade,
        },
    };

    struct Display;

    impl<T> Preset<T> for Display
    where
        T: fmt::Display,
    {
        type Upgrade = NeverUpgrade;
        type Error = tsukuyomi::Error;
        type Respond = DisplayRespond<T>;

        fn respond(this: T) -> Self::Respond {
            DisplayRespond(this)
        }
    }

    struct DisplayRespond<T>(T);

    impl<T> TryFuture for DisplayRespond<T>
    where
        T: fmt::Display,
    {
        type Ok = Response;
        type Error = tsukuyomi::Error;

        fn poll_ready(&mut self, _: &mut Input<'_>) -> Poll<Self::Ok, Self::Error> {
            Ok(http::Response::builder()
                .header("content-type", "text/plain; charset=utf-8")
                .body(self.0.to_string().into())
                .unwrap()
                .into())
        }
    }

    #[derive(tsukuyomi::output::Responder)]
    #[response(preset = "Display")]
    struct Foo(String);

    impl fmt::Display for Foo {
        fn fmt(&self, f: &mut fmt::Formatter) -> fmt::Result {
            self.0.fmt(f)
        }
    }

    #[derive(tsukuyomi::output::Responder)]
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
