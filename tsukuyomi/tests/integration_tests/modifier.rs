use {
    std::sync::{Arc, Mutex},
    tsukuyomi::{
        app::{route, scope, scope::Modifier},
        output::Output,
        AsyncResult,
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
    assert!(marker.lock().unwrap().is_empty());

    Ok(())
}

#[test]
fn global_modifiers() -> tsukuyomi::test::Result<()> {
    let marker = Arc::new(Mutex::new(vec![]));

    let mut server = tsukuyomi::app()
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

    let mut server = tsukuyomi::app()
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

    let mut server = tsukuyomi::app()
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
