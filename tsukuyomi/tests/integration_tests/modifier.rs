use {
    std::sync::{Arc, Mutex},
    tsukuyomi::{
        app::config::prelude::*, //
        chain,
        endpoint,
        future::MaybeFuture,
        handler::{AllowedMethods, Handler, ModifyHandler},
        server::Server,
        App,
    },
};

#[derive(Clone)]
struct MockModifier {
    marker: Arc<Mutex<Vec<&'static str>>>,
    name: &'static str,
}

impl<H: Handler> ModifyHandler<H> for MockModifier {
    type Output = H::Output;
    type Handler = MockHandler<H>;

    fn modify(&self, inner: H) -> Self::Handler {
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
    type Future = H::Future;

    fn allowed_methods(&self) -> Option<&AllowedMethods> {
        self.inner.allowed_methods()
    }

    fn call(&self, input: &mut tsukuyomi::Input<'_>) -> MaybeFuture<Self::Future> {
        self.marker.lock().unwrap().push(self.name);
        self.inner.call(input)
    }
}

#[test]
fn global_modifier() -> tsukuyomi::test::Result<()> {
    let marker = Arc::new(Mutex::new(vec![]));

    let mut server = App::configure(with_modifier(
        MockModifier {
            marker: marker.clone(),
            name: "M",
        },
        route().to(endpoint::any().say("")), //
    ))
    .map(Server::new)?
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

    let mut server = App::configure(with_modifier(
        chain![
            MockModifier {
                marker: marker.clone(),
                name: "M1",
            },
            MockModifier {
                marker: marker.clone(),
                name: "M2",
            }
        ],
        route().to(endpoint::any().say("")),
    ))
    .map(Server::new)?
    .into_test_server()?;

    let _ = server.perform("/")?;
    assert_eq!(*marker.lock().unwrap(), vec!["M2", "M1"]);

    Ok(())
}

#[test]
fn scoped_modifier() -> tsukuyomi::test::Result<()> {
    let marker = Arc::new(Mutex::new(vec![]));

    let mut server = App::configure(with_modifier(
        MockModifier {
            marker: marker.clone(),
            name: "M1",
        },
        chain![
            mount(
                "/path1",
                with_modifier(
                    MockModifier {
                        marker: marker.clone(),
                        name: "M2",
                    },
                    route().to(endpoint::any().say(""))
                )
            ), //
            route().segment("path2")?.to(endpoint::any().say("")),
        ],
    ))
    .map(Server::new)?
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

    let mut server = App::configure({
        mount(
            "/path",
            with_modifier(
                MockModifier {
                    marker: marker.clone(),
                    name: "M1",
                },
                mount(
                    "/to",
                    with_modifier(
                        MockModifier {
                            marker: marker.clone(),
                            name: "M2",
                        },
                        chain![
                            route().to(endpoint::any().say("")),
                            mount(
                                "/a",
                                with_modifier(
                                    MockModifier {
                                        marker: marker.clone(),
                                        name: "M3",
                                    },
                                    route().to(endpoint::any().say(""))
                                )
                            ) //
                        ],
                    ),
                ),
            ),
        )
    })
    .map(Server::new)?
    .into_test_server()?;

    let _ = server.perform("/path/to")?;
    assert_eq!(*marker.lock().unwrap(), vec!["M2", "M1"]);

    marker.lock().unwrap().clear();
    let _ = server.perform("/path/to/a")?;
    assert_eq!(*marker.lock().unwrap(), vec!["M3", "M2", "M1"]);

    Ok(())
}
