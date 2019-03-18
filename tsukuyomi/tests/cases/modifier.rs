use {
    std::sync::{Arc, Mutex},
    tsukuyomi::{
        config::prelude::*, //
        handler::{metadata::Metadata, Handler, ModifyHandler},
        test::{self, TestServer},
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
    type Error = H::Error;
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

    fn metadata(&self) -> Metadata {
        self.inner.metadata()
    }

    fn handle(&self) -> Self::Handle {
        self.marker.lock().unwrap().push(self.name);
        self.inner.handle()
    }
}

#[test]
fn global_modifier() -> test::Result {
    let marker = Arc::new(Mutex::new(vec![]));

    let app = App::create(
        path!("/") //
            .to(endpoint::reply(""))
            .modify(MockModifier {
                marker: marker.clone(),
                name: "M",
            }),
    )?;

    let mut server = TestServer::new(app)?;
    let mut client = server.connect();

    client.get("/");
    assert_eq!(*marker.lock().unwrap(), vec!["M"]);

    marker.lock().unwrap().clear();
    client.get("/noroute");
    assert!(marker.lock().unwrap().is_empty());

    Ok(())
}

#[test]
fn global_modifiers() -> test::Result {
    let marker = Arc::new(Mutex::new(vec![]));

    let app = App::create(
        path!("/") //
            .to(endpoint::reply(""))
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

    let mut server = TestServer::new(app)?;
    let mut client = server.connect();

    client.get("/");
    assert_eq!(*marker.lock().unwrap(), vec!["M2", "M1"]);

    Ok(())
}

#[test]
fn scoped_modifier() -> test::Result {
    let marker = Arc::new(Mutex::new(vec![]));

    let app = App::create(
        chain![
            mount("/path1").with({
                path!("/") //
                    .to(endpoint::reply(""))
                    .modify(MockModifier {
                        marker: marker.clone(),
                        name: "M2",
                    })
            }), //
            path!("/path2").to(endpoint::reply("")),
        ]
        .modify(MockModifier {
            marker: marker.clone(),
            name: "M1",
        }),
    )?;

    let mut server = TestServer::new(app)?;
    let mut client = server.connect();

    client.get("/path1");
    assert_eq!(*marker.lock().unwrap(), vec!["M2", "M1"]);

    marker.lock().unwrap().clear();
    client.get("/path2");
    assert_eq!(*marker.lock().unwrap(), vec!["M1"]);

    Ok(())
}

#[test]
fn nested_modifiers() -> test::Result {
    let marker = Arc::new(Mutex::new(vec![]));

    let app = App::create({
        mount("/path").with({
            mount("/to")
                .with(
                    chain![
                        path!("/").to(endpoint::reply("")),
                        mount("/a").with({
                            path!("/") //
                                .to(endpoint::reply(""))
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

    let mut server = TestServer::new(app)?;
    let mut client = server.connect();

    client.get("/path/to");
    assert_eq!(*marker.lock().unwrap(), vec!["M2", "M1"]);

    marker.lock().unwrap().clear();
    client.get("/path/to/a");
    assert_eq!(*marker.lock().unwrap(), vec!["M3", "M2", "M1"]);

    Ok(())
}
