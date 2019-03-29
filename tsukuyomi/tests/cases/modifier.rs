use {
    std::sync::{Arc, Mutex},
    tsukuyomi::{
        chain, endpoint,
        handler::{Handler, ModifyHandler},
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

    fn handle(&self) -> Self::Handle {
        self.marker.lock().unwrap().push(self.name);
        self.inner.handle()
    }
}

#[test]
fn global_modifier() -> test::Result {
    let marker = Arc::new(Mutex::new(vec![]));

    let app = App::build(|mut s| {
        let m = MockModifier {
            marker: marker.clone(),
            name: "M",
        };
        s.at("/")?.with(m).to(endpoint::call(|| "")) //
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

    let app = App::build(|mut s| {
        s.at("/")?.with(m).to(endpoint::call(|| "")) //
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

    let app = App::build(|mut s| {
        s.with(&m1).done(|mut s| {
            s.mount("/path1")?
                .at("/")?
                .with(m2)
                .to(endpoint::call(|| ""))?;

            s.at("/path2")?.to(endpoint::call(|| ""))
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

    let app = App::build(|mut s| {
        s.mount("/path")?
            .mount("/to")?
            .with(m1)
            .with(m2)
            .done(|mut s| {
                s.at("/")?.to(endpoint::call(|| ""))?;
                s.mount("/a")?.at("/")?.with(m3).to(endpoint::call(|| "")) //
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
