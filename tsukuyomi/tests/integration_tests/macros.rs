mod responder {
    use std::fmt;
    use tsukuyomi::test::test_server;

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

    #[derive(tsukuyomi::output::Responder)]
    #[responder(respond_to = "self::sub::respond_to")]
    struct Foo(String);

    impl fmt::Display for Foo {
        fn fmt(&self, f: &mut fmt::Formatter) -> fmt::Result {
            self.0.fmt(f)
        }
    }

    #[test]
    fn test_custom_responder() {
        let mut server = test_server({
            tsukuyomi::app(|scope| {
                scope.route(tsukuyomi::route::index().reply(|| Foo("Foo".into())));
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

    #[test]
    #[ignore]
    #[allow(dead_code)]
    fn compiletest_derive_responder_enum() {
        use tsukuyomi::output::Responder;

        fn assert_impl_responder<T: Responder>(t: T) {
            drop(t);
        }

        #[derive(Responder)]
        enum Single {
            Foo,
        }

        #[derive(Responder)]
        enum WithValue {
            A,
            Foo(String),
        }

        #[derive(Responder)]
        enum Multi {
            Foo(String),
            Single(Single),
            WithValue { a: WithValue },
        }

        assert_impl_responder(Single::Foo);
        assert_impl_responder(WithValue::Foo("foo".into()));
        assert_impl_responder(Multi::Single(Single::Foo));
    }
}
