extern crate futures;
extern crate http;
extern crate tsukuyomi;

use tsukuyomi::local::LocalServer;
use tsukuyomi::modifier::{AfterHandle, BeforeHandle, Modifier};
use tsukuyomi::output::Output;
use tsukuyomi::{App, Input};

use http::Response;
use std::sync::atomic::{AtomicUsize, Ordering};
use std::sync::Arc;

#[derive(Default, Clone)]
struct Marker(Arc<AtomicUsize>);

impl Marker {
    fn mark(&self) {
        self.0.fetch_add(1, Ordering::SeqCst);
    }

    fn count(&self) -> usize {
        self.0.load(Ordering::SeqCst)
    }
}

/// A modifier which marks the method calls and does not anything else.
struct EmptyModifier {
    before: Marker,
    after: Marker,
}

impl Modifier for EmptyModifier {
    fn before_handle(&self, _: &mut Input) -> BeforeHandle {
        self.before.mark();
        BeforeHandle::ok()
    }

    fn after_handle(&self, _: &mut Input, output: Output) -> AfterHandle {
        self.after.mark();
        AfterHandle::ok(output)
    }
}

/// A modifier which marks the method calls and returns a `done(output)` when `before_handle()`
/// called.
struct BeforeDoneModifier {
    before: Marker,
    after: Marker,
}

impl Modifier for BeforeDoneModifier {
    fn before_handle(&self, _: &mut Input) -> BeforeHandle {
        self.before.mark();
        BeforeHandle::done(Response::new(()))
    }

    fn after_handle(&self, _: &mut Input, output: Output) -> AfterHandle {
        self.after.mark();
        AfterHandle::ok(output)
    }
}

/// Creates a handler function which marks a function call.
fn handler(marker: &Marker) -> impl Fn(&mut Input) -> &'static str + Send + 'static {
    let marker = marker.clone();
    move |_| {
        marker.mark();
        "dummy"
    }
}

#[test]
fn single_empty_modifier() {
    let before = Marker::default();
    let after = Marker::default();
    let handle = Marker::default();

    let app = App::builder()
        .mount("/", |m| {
            m.get("/").handle(handler(&handle));
        })
        .modifier(EmptyModifier {
            before: before.clone(),
            after: after.clone(),
        })
        .finish()
        .unwrap();

    let mut server = LocalServer::new(app).unwrap();

    let _ = server.client().get("/").execute().unwrap();

    assert_eq!(before.count(), 1);
    assert_eq!(after.count(), 1);
    assert_eq!(handle.count(), 1);
}

#[test]
fn single_before_done_modifier() {
    let before = Marker::default();
    let after = Marker::default();
    let handle = Marker::default();

    let app = App::builder()
        .mount("/", |m| {
            m.get("/").handle(handler(&handle));
        })
        .modifier(BeforeDoneModifier {
            before: before.clone(),
            after: after.clone(),
        })
        .finish()
        .unwrap();

    let mut server = LocalServer::new(app).unwrap();

    let _ = server.client().get("/").execute().unwrap();

    assert_eq!(before.count(), 1);
    assert_eq!(after.count(), 0);
    assert_eq!(handle.count(), 0);
}

#[test]
fn multiple_empty_modifier() {
    let before1 = Marker::default();
    let before2 = Marker::default();
    let before3 = Marker::default();
    let after1 = Marker::default();
    let after2 = Marker::default();
    let after3 = Marker::default();
    let handle = Marker::default();

    let app = App::builder()
        .mount("/", |m| {
            m.get("/").handle(handler(&handle));
        })
        .modifier(EmptyModifier {
            before: before1.clone(),
            after: after1.clone(),
        })
        .modifier(EmptyModifier {
            before: before2.clone(),
            after: after2.clone(),
        })
        .modifier(EmptyModifier {
            before: before3.clone(),
            after: after3.clone(),
        })
        .finish()
        .unwrap();

    let mut server = LocalServer::new(app).unwrap();

    let _ = server.client().get("/").execute().unwrap();

    assert_eq!(before1.count(), 1);
    assert_eq!(before2.count(), 1);
    assert_eq!(before3.count(), 1);
    assert_eq!(after1.count(), 1);
    assert_eq!(after2.count(), 1);
    assert_eq!(after3.count(), 1);
    assert_eq!(handle.count(), 1);
}

#[test]
fn empty_and_before_done_modifier_1() {
    let before1 = Marker::default();
    let before2 = Marker::default();
    let after1 = Marker::default();
    let after2 = Marker::default();
    let handle = Marker::default();

    let app = App::builder()
        .mount("/", |m| {
            m.get("/").handle(handler(&handle));
        })
        .modifier(EmptyModifier {
            before: before1.clone(),
            after: after1.clone(),
        })
        .modifier(BeforeDoneModifier {
            before: before2.clone(),
            after: after2.clone(),
        })
        .finish()
        .unwrap();

    let mut server = LocalServer::new(app).unwrap();

    let _ = server.client().get("/").execute().unwrap();

    assert_eq!(before1.count(), 1);
    assert_eq!(before2.count(), 1);
    assert_eq!(after1.count(), 1);
    assert_eq!(after2.count(), 0);
    assert_eq!(handle.count(), 0);
}

#[test]
fn empty_and_before_done_modifier_2() {
    let before1 = Marker::default();
    let before2 = Marker::default();
    let after1 = Marker::default();
    let after2 = Marker::default();
    let handle = Marker::default();

    let app = App::builder()
        .mount("/", |m| {
            m.get("/").handle(handler(&handle));
        })
        .modifier(BeforeDoneModifier {
            before: before1.clone(),
            after: after1.clone(),
        })
        .modifier(EmptyModifier {
            before: before2.clone(),
            after: after2.clone(),
        })
        .finish()
        .unwrap();

    let mut server = LocalServer::new(app).unwrap();

    let _ = server.client().get("/").execute().unwrap();

    assert_eq!(before1.count(), 1);
    assert_eq!(before2.count(), 0);
    assert_eq!(after1.count(), 0);
    assert_eq!(after2.count(), 0);
    assert_eq!(handle.count(), 0);
}
