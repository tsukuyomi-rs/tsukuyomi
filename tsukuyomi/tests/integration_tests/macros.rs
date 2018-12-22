mod responder {
    use {
        std::fmt,
        tsukuyomi::{
            config::prelude::*, //
            App,
        },
        tsukuyomi_server::test::ResponseExt,
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

    mod sub {
        use {
            http::{Request, Response},
            tsukuyomi::util::Never,
        };

        pub fn display<T>(this: T, _: &Request<()>) -> Result<Response<String>, Never>
        where
            T: std::fmt::Display,
        {
            Ok(Response::builder()
                .header("content-type", "text/plain; charset=utf-8")
                .body(this.to_string())
                .unwrap())
        }
    }

    #[test]
    fn test_responder() -> tsukuyomi_server::Result<()> {
        #[derive(tsukuyomi::output::IntoResponse)]
        #[response(with = "self::sub::display")]
        struct Foo(String);

        impl fmt::Display for Foo {
            fn fmt(&self, f: &mut fmt::Formatter) -> fmt::Result {
                self.0.fmt(f)
            }
        }

        let app = App::create({
            path!("/") //
                .to(endpoint::call(|| Foo("Foo".into())))
        })?;

        let mut server = tsukuyomi_server::test::server(app)?;

        let response = server.perform("/")?;
        assert_eq!(response.status(), 200);
        assert_eq!(
            response.header("content-type")?,
            "text/plain; charset=utf-8"
        );
        assert_eq!(response.body().to_utf8()?, "Foo");

        Ok(())
    }
}
