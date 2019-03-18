use {
    http::{header, StatusCode},
    tsukuyomi::{
        config::prelude::*, //
        test::{self, loc, TestServer},
        App,
    },
};

fn assert_impl_into_response<T: tsukuyomi::output::IntoResponse>() {}

#[test]
#[ignore]
#[allow(dead_code)]
fn compiletest_struct() {
    use tsukuyomi::output::IntoResponse;

    #[derive(IntoResponse)]
    struct Unit;

    #[derive(IntoResponse)]
    struct NewType(String);

    #[derive(IntoResponse)]
    struct SingleField {
        inner: String,
    }

    #[derive(IntoResponse)]
    struct Generic<T>(T);

    assert_impl_into_response::<Unit>();
    assert_impl_into_response::<NewType>();
    assert_impl_into_response::<SingleField>();
    assert_impl_into_response::<Generic<Unit>>();
}

#[test]
#[ignore]
#[allow(dead_code)]
fn compiletest_enum() {
    use tsukuyomi::output::IntoResponse;

    #[derive(IntoResponse)]
    enum Never {}

    #[derive(IntoResponse)]
    enum Unit {
        Foo,
    }

    #[derive(IntoResponse)]
    enum Unnamed {
        Foo(String),
    }

    #[derive(IntoResponse)]
    enum Named {
        Foo { inner: String },
    }

    #[derive(IntoResponse)]
    enum Complex {
        Unit,
        Unnamed(()),
        Named { field: String },
    }

    #[derive(IntoResponse)]
    enum Either<L, R> {
        Left(L),
        Right(R),
    }

    assert_impl_into_response::<Never>();
    assert_impl_into_response::<Unit>();
    assert_impl_into_response::<Unnamed>();
    assert_impl_into_response::<Named>();
    assert_impl_into_response::<Complex>();
    assert_impl_into_response::<Either<Never, Never>>();
}

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
        type Body = String;
        type Error = tsukuyomi::util::Never;

        fn into_response(this: T, _: &Request<()>) -> Result<Response<Self::Body>, Self::Error> {
            Ok(Response::builder()
                .header("content-type", "text/plain; charset=utf-8")
                .body(this.to_string())
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
