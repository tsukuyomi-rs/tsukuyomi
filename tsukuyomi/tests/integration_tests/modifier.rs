use {
    http::Response,
    std::sync::{Arc, Mutex},
    tsukuyomi::{
        handler::AsyncResult,
        output::{Output, ResponseBody},
        route, scope, Modifier,
    },
};

#[derive(Clone)]
struct MockModifier {
    marker: Arc<Mutex<Vec<&'static str>>>,
    name: &'static str,
}

impl Modifier for MockModifier {
    fn modify(&self, result: AsyncResult<Output>) -> AsyncResult<Output> {
        self.marker.lock().unwrap().push(self.name);
        result
    }
}

#[test]
fn global_modifier() -> tsukuyomi::test::Result<()> {
    let marker = Arc::new(Mutex::new(vec![]));

    let mut server = tsukuyomi::app!()
        .route(route!("/").reply(|| "")) //
        .modifier(MockModifier {
            marker: marker.clone(),
            name: "M",
        }) //
        .build_server()?
        .into_test_server()?;

    let _ = server.perform("/")?;
    assert_eq!(*marker.lock().unwrap(), vec!["M"]);

    marker.lock().unwrap().clear();
    let _ = server.perform("/dummy")?;
    assert_eq!(*marker.lock().unwrap(), vec!["M"]);

    Ok(())
}

#[test]
fn global_modifiers() -> tsukuyomi::test::Result<()> {
    let marker = Arc::new(Mutex::new(vec![]));

    let mut server = tsukuyomi::app!()
        .route(route!().reply(|| ""))
        .modifier(MockModifier {
            marker: marker.clone(),
            name: "M1",
        }) //
        .modifier(MockModifier {
            marker: marker.clone(),
            name: "M2",
        }) //
        .build_server()?
        .into_test_server()?;

    let _ = server.perform("/")?;
    assert_eq!(*marker.lock().unwrap(), vec!["M2", "M1"]);

    Ok(())
}

#[test]
fn scoped_modifier() -> tsukuyomi::test::Result<()> {
    let marker = Arc::new(Mutex::new(vec![]));

    let mut server = tsukuyomi::app!()
        .modifier(MockModifier {
            marker: marker.clone(),
            name: "M1",
        }) //
        .mount(
            scope!("/path1")
                .modifier(MockModifier {
                    marker: marker.clone(),
                    name: "M2",
                }) //
                .route(route!("/").reply(|| "")),
        ) //
        .route(route!("/path2").reply(|| ""))
        .build_server()?
        .into_test_server()?;

    let _ = server.perform("/path1")?;
    assert_eq!(*marker.lock().unwrap(), vec!["M2", "M1"]);

    marker.lock().unwrap().clear();
    let _ = server.perform("/path2")?;
    assert_eq!(*marker.lock().unwrap(), vec!["M1"]);

    Ok(())
}

#[test]
fn nested_modifiers() -> tsukuyomi::test::Result<()> {
    let marker = Arc::new(Mutex::new(vec![]));

    let mut server = tsukuyomi::app!()
        .mount(
            scope!("/path")
                .modifier(MockModifier {
                    marker: marker.clone(),
                    name: "M1",
                }) //
                .mount(
                    scope!("/to")
                        .modifier(MockModifier {
                            marker: marker.clone(),
                            name: "M2",
                        }) //
                        .route(route!().reply(|| ""))
                        .mount(
                            scope!("/a")
                                .modifier(MockModifier {
                                    marker: marker.clone(),
                                    name: "M3",
                                }) //
                                .route(route!().reply(|| "")),
                        ),
                ),
        ) //
        .build_server()?
        .into_test_server()?;

    let _ = server.perform("/path/to")?;
    assert_eq!(*marker.lock().unwrap(), vec!["M2", "M1"]);

    marker.lock().unwrap().clear();
    let _ = server.perform("/path/to/a")?;
    assert_eq!(*marker.lock().unwrap(), vec!["M3", "M2", "M1"]);

    Ok(())
}

#[test]
fn setup() -> tsukuyomi::test::Result<()> {
    use tsukuyomi::app::scope::Context;

    struct SetState(Option<String>);
    impl Modifier for SetState {
        fn setup(&mut self, cx: &mut Context<'_>) -> tsukuyomi::app::Result<()> {
            cx.set_state(self.0.take().unwrap());
            Ok(())
        }

        fn modify(&self, handle: AsyncResult<Output>) -> AsyncResult<Output> {
            handle
        }
    }

    let mut server = tsukuyomi::app!()
        .modifier(SetState(Some("foo".into())))
        .route(
            route!("/") //
                .raw(|| {
                    AsyncResult::ready(|input| {
                        assert_eq!(input.states.get::<String>(), "foo");
                        Ok(Response::new(ResponseBody::default()))
                    })
                }),
        ) //
        .mount(
            scope!("/sub").modifier(SetState(Some("bar".into()))).route(
                route!("/") //
                    .raw(|| {
                        AsyncResult::ready(|input| {
                            assert_eq!(input.states.get::<String>(), "bar");
                            Ok(Response::new(ResponseBody::default()))
                        })
                    }),
            ),
        ) //
        .build_server()?
        .into_test_server()?;

    let _ = server.perform("/")?;
    let _ = server.perform("/sub")?;

    Ok(())
}
