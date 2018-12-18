use {
    std::sync::{Arc, Mutex},
    tsukuyomi::{
        config::prelude::*, //
        handler::{AllowedMethods, Handler, ModifyHandler},
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
    type Error = H::Error;
    type Handle = H::Handle;

    fn allowed_methods(&self) -> Option<&AllowedMethods> {
        self.inner.allowed_methods()
    }

    fn handle(&self) -> Self::Handle {
        self.marker.lock().unwrap().push(self.name);
        self.inner.handle()
    }
}

#[test]
fn global_modifier() -> tsukuyomi::test::Result<()> {
    let marker = Arc::new(Mutex::new(vec![]));

    let app = App::create(
        path!("/") //
            .to(endpoint::any().reply(""))
            .modify(MockModifier {
                marker: marker.clone(),
                name: "M",
            }),
    )?;

    let mut server = tsukuyomi::test::server(app)?;

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

    let app = App::create(
        path!("/") //
            .to(endpoint::any().reply(""))
            .modify(chain![
                MockModifier {
                    marker: marker.clone(),
                    name: "M1",
                },
                MockModifier {
                    marker: marker.clone(),
                    name: "M2",
                }
            ]),
    )?;
    let mut server = tsukuyomi::test::server(app)?;

    let _ = server.perform("/")?;
    assert_eq!(*marker.lock().unwrap(), vec!["M2", "M1"]);

    Ok(())
}

#[test]
fn scoped_modifier() -> tsukuyomi::test::Result<()> {
    let marker = Arc::new(Mutex::new(vec![]));

    let app = App::create(
        chain![
            mount("/path1").with({
                path!("/") //
                    .to(endpoint::any().reply(""))
                    .modify(MockModifier {
                        marker: marker.clone(),
                        name: "M2",
                    })
            }), //
            path!("/path2").to(endpoint::any().reply("")),
        ]
        .modify(MockModifier {
            marker: marker.clone(),
            name: "M1",
        }),
    )?;
    let mut server = tsukuyomi::test::server(app)?;

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

    let app = App::create({
        mount("/path").with({
            mount("/to")
                .with(
                    chain![
                        path!("/").to(endpoint::any().reply("")),
                        mount("/a").with({
                            path!("/") //
                                .to(endpoint::any().reply(""))
                                .modify(MockModifier {
                                    marker: marker.clone(),
                                    name: "M3",
                                })
                        })
                    ]
                    .modify(MockModifier {
                        marker: marker.clone(),
                        name: "M2",
                    }),
                )
                .modify(MockModifier {
                    marker: marker.clone(),
                    name: "M1",
                })
        })
    })?;
    let mut server = tsukuyomi::test::server(app)?;

    let _ = server.perform("/path/to")?;
    assert_eq!(*marker.lock().unwrap(), vec!["M2", "M1"]);

    marker.lock().unwrap().clear();
    let _ = server.perform("/path/to/a")?;
    assert_eq!(*marker.lock().unwrap(), vec!["M3", "M2", "M1"]);

    Ok(())
}
