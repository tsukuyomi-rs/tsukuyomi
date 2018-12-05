use {
    std::sync::{Arc, Mutex},
    tsukuyomi::{
        app::directives::*, //
        Handler,
        MaybeFuture,
        Modifier,
    },
};

#[derive(Clone)]
struct MockModifier {
    marker: Arc<Mutex<Vec<&'static str>>>,
    name: &'static str,
}

impl<H> Modifier<H> for MockModifier
where
    H: Handler + Send + Sync + 'static,
{
    type Out = MockHandler<H>;

    fn modify(&self, inner: H) -> Self::Out {
        MockHandler {
            inner,
            marker: self.marker.clone(),
            name: self.name,
        }
    }
}

struct MockHandler<H> {
    inner: H,
    marker: Arc<Mutex<Vec<&'static str>>>,
    name: &'static str,
}

impl<H> Handler for MockHandler<H>
where
    H: Handler,
{
    type Output = H::Output;
    type Error = H::Error;
    type Future = H::Future;

    fn call(&self, input: &mut tsukuyomi::Input<'_>) -> MaybeFuture<Self::Future> {
        self.marker.lock().unwrap().push(self.name);
        self.inner.call(input)
    }
}

#[test]
fn global_modifier() -> tsukuyomi::test::Result<()> {
    let marker = Arc::new(Mutex::new(vec![]));

    let mut server = App::builder()
        .with(route("/")?.say("")) //
        .modifier(MockModifier {
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
        .with(route("/")?.say(""))
        .modifier(MockModifier {
            marker: marker.clone(),
            name: "M1",
        }).modifier(MockModifier {
            marker: marker.clone(),
            name: "M2",
        }) //
        .build_server()?
        .into_test_server()?;

    let _ = server.perform("/")?;
    assert_eq!(*marker.lock().unwrap(), vec!["M1", "M2"]);

    Ok(())
}

#[test]
fn scoped_modifier() -> tsukuyomi::test::Result<()> {
    let marker = Arc::new(Mutex::new(vec![]));

    let mut server = App::builder()
        .modifier(MockModifier {
            marker: marker.clone(),
            name: "M1",
        }) //
        .with(
            mount("/path1")?
                .modifier(MockModifier {
                    marker: marker.clone(),
                    name: "M2",
                }) //
                .with(route("/")?.say("")),
        ) //
        .with(route("/path2")?.say(""))
        .build_server()?
        .into_test_server()?;

    let _ = server.perform("/path1")?;
    assert_eq!(*marker.lock().unwrap(), vec!["M1", "M2"]);

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
            mount("/path")?
                .modifier(MockModifier {
                    marker: marker.clone(),
                    name: "M1",
                }) //
                .with(
                    mount("/to")?
                        .modifier(MockModifier {
                            marker: marker.clone(),
                            name: "M2",
                        }) //
                        .with(route("/")?.say(""))
                        .with(
                            mount("/a")?
                                .modifier(MockModifier {
                                    marker: marker.clone(),
                                    name: "M3",
                                }) //
                                .with(route("/")?.say("")),
                        ),
                ),
        ) //
        .build_server()?
        .into_test_server()?;

    let _ = server.perform("/path/to")?;
    assert_eq!(*marker.lock().unwrap(), vec!["M1", "M2"]);

    marker.lock().unwrap().clear();
    let _ = server.perform("/path/to/a")?;
    assert_eq!(*marker.lock().unwrap(), vec!["M1", "M2", "M3"]);

    Ok(())
}
