mod responder {
    use std::fmt;
    use tsukuyomi::test::test_server;

    fn assert_impl_responder<T: tsukuyomi::output::Responder>() {}

    #[test]
    #[ignore]
    #[allow(dead_code)]
    fn compiletest_struct() {
        use tsukuyomi::output::Responder;

        #[derive(Responder)]
        struct Unit;

        #[derive(Responder)]
        struct NewType(String);

        #[derive(Responder)]
        struct SingleField {
            inner: String,
        }

        assert_impl_responder::<Unit>();
        assert_impl_responder::<NewType>();
        assert_impl_responder::<SingleField>();
    }

    #[test]
    #[ignore]
    #[allow(dead_code)]
    fn compiletest_enum() {
        use tsukuyomi::output::Responder;

        #[derive(Responder)]
        enum Never {}

        #[derive(Responder)]
        enum Unit {
            Foo,
        }

        #[derive(Responder)]
        enum Unnamed {
            Foo(String),
        }

        #[derive(Responder)]
        enum Named {
            Foo { inner: String },
        }

        #[derive(Responder)]
        enum Complex {
            Unit,
            Unnamed(()),
            Named { field: String },
        }

        assert_impl_responder::<Never>();
        assert_impl_responder::<Unit>();
        assert_impl_responder::<Unnamed>();
        assert_impl_responder::<Named>();
        assert_impl_responder::<Complex>();
    }

    mod sub {
        use http::Response;
        use tsukuyomi::error::Never;
        use tsukuyomi::input::Input;

        pub fn respond_to<T>(this: T, _: &mut Input<'_>) -> Result<Response<String>, Never>
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
    fn test_responder() {
        #[derive(tsukuyomi::output::Responder)]
        #[responder(respond_to = "self::sub::respond_to")]
        struct Foo(String);

        impl fmt::Display for Foo {
            fn fmt(&self, f: &mut fmt::Formatter) -> fmt::Result {
                self.0.fmt(f)
            }
        }

        let mut server = test_server({
            tsukuyomi::app(|scope| {
                scope.route(tsukuyomi::app::Route::index().reply(|| Foo("Foo".into())));
            }).unwrap()
        });
        let response = server.perform(http::Request::get("/")).unwrap();
        assert_eq!(response.status(), 200);
        assert_eq!(
            response.headers().get("content-type").unwrap(),
            "text/plain; charset=utf-8"
        );
        assert_eq!(response.body().to_utf8().unwrap(), "Foo");
    }
}
