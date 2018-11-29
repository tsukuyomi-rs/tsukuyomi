use {
    std::sync::{Arc, Mutex},
    tsukuyomi::{
        app::{
            scope::{mount, route},
            App,
        },
        handler::AsyncResult,
        output::Output,
        uri, Modifier,
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

impl tsukuyomi::app::Scope for MockModifier {
    type Error = tsukuyomi::Never;

    fn configure(self, cx: &mut tsukuyomi::app::scope::Context<'_>) -> Result<(), Self::Error> {
        tsukuyomi::app::scope::modifier(self).configure(cx)
    }
}

#[test]
fn global_modifier() -> tsukuyomi::test::Result<()> {
    let marker = Arc::new(Mutex::new(vec![]));

    let mut server = App::builder()
        .with(
            route!("/") //
                .reply(|| ""),
        ) //
        .with(MockModifier {
            marker: marker.clone(),
            name: "M",
        }) //
        .build_server()?
        .into_test_server()?;

    let _ = server.perform("/")?;
    assert_eq!(*marker.lock().unwrap(), vec!["M"]);

    marker.lock().unwrap().clear();
    let _ = server.perform("/noroute")?;
    assert!(marker.lock().unwrap().is_empty());

    Ok(())
}

#[test]
fn global_modifiers() -> tsukuyomi::test::Result<()> {
    let marker = Arc::new(Mutex::new(vec![]));

    let mut server = App::builder()
        .with(
            route!() //
                .reply(|| ""),
        ).with(MockModifier {
            marker: marker.clone(),
            name: "M1",
        }) //
        .with(MockModifier {
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

    let mut server = App::builder()
        .with(MockModifier {
            marker: marker.clone(),
            name: "M1",
        }) //
        .with(
            mount(uri!("/path1"))
                .with(MockModifier {
                    marker: marker.clone(),
                    name: "M2",
                }) //
                .with(route!("/").reply(|| "")),
        ) //
        .with(route!("/path2").reply(|| ""))
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

    let mut server = App::builder()
        .with(
            mount(uri!("/path"))
                .with(MockModifier {
                    marker: marker.clone(),
                    name: "M1",
                }) //
                .with(
                    mount(uri!("/to"))
                        .with(MockModifier {
                            marker: marker.clone(),
                            name: "M2",
                        }) //
                        .with(route!().reply(|| ""))
                        .with(
                            mount(uri!("/a"))
                                .with(MockModifier {
                                    marker: marker.clone(),
                                    name: "M3",
                                }) //
                                .with(route!().reply(|| "")),
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
