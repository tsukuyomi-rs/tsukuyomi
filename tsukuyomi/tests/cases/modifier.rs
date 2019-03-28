use {
    std::sync::{Arc, Mutex},
    tsukuyomi::{
        chain,
        endpoint::builder as endpoint,
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

    let app = App::build(|s| {
        let m = MockModifier {
            marker: marker.clone(),
            name: "M",
        };
        s.at("/", m, endpoint::reply("")) //
    })?;
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

    let m1 = MockModifier {
        marker: marker.clone(),
        name: "M1",
    };
    let m2 = MockModifier {
        marker: marker.clone(),
        name: "M2",
    };
    let m = chain!(m1, m2);

    let app = App::build(|s| {
        s.at("/", m, endpoint::reply("")) //
    })?;
    let mut server = TestServer::new(app)?;
    let mut client = server.connect();

    client.get("/");
    assert_eq!(*marker.lock().unwrap(), vec!["M2", "M1"]);

    Ok(())
}

#[test]
fn scoped_modifier() -> test::Result {
    let marker = Arc::new(Mutex::new(vec![]));

    let m1 = MockModifier {
        marker: marker.clone(),
        name: "M1",
    };
    let m2 = MockModifier {
        marker: marker.clone(),
        name: "M2",
    };

    let app = App::build(|s| {
        s.with(&m1, |s| {
            s.nest("/path1", &(), |s| {
                s.at("/", m2, endpoint::reply("")) //
            })?;
            s.at("/path2", (), endpoint::reply(""))
        })
    })?;
    let mut server = TestServer::new(app)?;
    let mut client = server.connect();

    client.get("/path1");
    assert_eq!(*marker.lock().unwrap(), vec!["M1", "M2"]);

    marker.lock().unwrap().clear();
    client.get("/path2");
    assert_eq!(*marker.lock().unwrap(), vec!["M1"]);

    Ok(())
}

#[test]
fn nested_modifiers() -> test::Result {
    let marker = Arc::new(Mutex::new(vec![]));
    let m1 = MockModifier {
        marker: marker.clone(),
        name: "M1",
    };
    let m2 = MockModifier {
        marker: marker.clone(),
        name: "M2",
    };
    let m3 = MockModifier {
        marker: marker.clone(),
        name: "M3",
    };

    let app = App::build(|s| {
        s.nest("/path", (), |s| {
            s.nest("/to", m1, |s| {
                s.with(m2, |s| {
                    s.at("/", (), endpoint::reply(""))?;
                    s.nest("/a", (), |s| {
                        s.at("/", m3, endpoint::reply("")) //
                    })
                })
            })
        })
    })?;
    let mut server = TestServer::new(app)?;
    let mut client = server.connect();

    client.get("/path/to");
    assert_eq!(*marker.lock().unwrap(), vec!["M1", "M2"]);

    marker.lock().unwrap().clear();
    client.get("/path/to/a");
    assert_eq!(*marker.lock().unwrap(), vec!["M1", "M2", "M3"]);

    Ok(())
}
