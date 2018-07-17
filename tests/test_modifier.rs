extern crate futures;
extern crate http;
extern crate tsukuyomi;

use tsukuyomi::handler::Handle;
use tsukuyomi::local::LocalServer;
use tsukuyomi::modifier::{AfterHandle, BeforeHandle, Modifier};
use tsukuyomi::output::Output;
use tsukuyomi::{App, Error, Input};

use http::Response;
use std::sync::{Arc, Mutex};

struct MarkModifier<T1, T2>
where
    T1: Fn(&mut Vec<&'static str>) -> Result<(), Error>,
    T2: Fn(&mut Vec<&'static str>) -> Result<Output, Error>,
{
    marker: Arc<Mutex<Vec<&'static str>>>,
    before: T1,
    after: T2,
}

impl<T1, T2> Modifier for MarkModifier<T1, T2>
where
    T1: Fn(&mut Vec<&'static str>) -> Result<(), Error>,
    T2: Fn(&mut Vec<&'static str>) -> Result<Output, Error>,
{
    fn before_handle(&self, _: &mut Input) -> BeforeHandle {
        match (self.before)(&mut *self.marker.lock().unwrap()) {
            Ok(()) => BeforeHandle::ok(),
            Err(err) => BeforeHandle::err(err),
        }
    }

    fn after_handle(&self, _: &mut Input, _: Output) -> AfterHandle {
        match (self.after)(&mut *self.marker.lock().unwrap()) {
            Ok(output) => AfterHandle::ok(output),
            Err(err) => AfterHandle::err(err),
        }
    }
}

#[test]
fn global_modifier() {
    let marker = Arc::new(Mutex::new(vec![]));

    let app = App::builder()
        .route(("/", {
            let marker = marker.clone();
            move |_: &mut Input| {
                marker.lock().unwrap().push("H");
                Handle::ok(Response::new(()).into())
            }
        }))
        .modifier(MarkModifier {
            marker: marker.clone(),
            before: |m| {
                m.push("B");
                Ok(())
            },
            after: |m| {
                m.push("A");
                Ok(Response::new(()).into())
            },
        })
        .finish()
        .unwrap();

    let mut server = LocalServer::new(app).unwrap();

    let _ = server.client().get("/").execute().unwrap();
    assert_eq!(*marker.lock().unwrap(), vec!["B", "H", "A"]);
}

#[test]
fn global_modifier_error_on_before() {
    let marker = Arc::new(Mutex::new(vec![]));

    let app = App::builder()
        .route(("/", {
            let marker = marker.clone();
            move |_: &mut Input| {
                marker.lock().unwrap().push("H");
                Handle::ok(Response::new(()).into())
            }
        }))
        .modifier(MarkModifier {
            marker: marker.clone(),
            before: |m| {
                m.push("B");
                Err(Error::not_found())
            },
            after: |m| {
                m.push("A");
                Ok(Response::new(()).into())
            },
        })
        .finish()
        .unwrap();

    let mut server = LocalServer::new(app).unwrap();

    let _ = server.client().get("/").execute().unwrap();
    assert_eq!(*marker.lock().unwrap(), vec!["B"]);
}

#[test]
fn global_modifiers() {
    let marker = Arc::new(Mutex::new(vec![]));

    let app = App::builder()
        .route(("/", {
            let marker = marker.clone();
            move |_: &mut Input| {
                marker.lock().unwrap().push("H");
                Handle::ok(Response::new(()).into())
            }
        }))
        .modifier(MarkModifier {
            marker: marker.clone(),
            before: |m| {
                m.push("B1");
                Ok(())
            },
            after: |m| {
                m.push("A1");
                Ok(Response::new(()).into())
            },
        })
        .modifier(MarkModifier {
            marker: marker.clone(),
            before: |m| {
                m.push("B2");
                Ok(())
            },
            after: |m| {
                m.push("A2");
                Ok(Response::new(()).into())
            },
        })
        .finish()
        .unwrap();

    let mut server = LocalServer::new(app).unwrap();

    let _ = server.client().get("/").execute().unwrap();
    assert_eq!(*marker.lock().unwrap(), vec!["B1", "B2", "H", "A2", "A1"]);
}

#[test]
fn scoped_modifier() {
    let marker = Arc::new(Mutex::new(vec![]));

    let app = App::builder()
        .modifier(MarkModifier {
            marker: marker.clone(),
            before: |m| {
                m.push("B1");
                Ok(())
            },
            after: |m| {
                m.push("A1");
                Ok(Response::new(()).into())
            },
        })
        .mount("/path1", |s| {
            s.modifier(MarkModifier {
                marker: marker.clone(),
                before: |m| {
                    m.push("B2");
                    Ok(())
                },
                after: |m| {
                    m.push("A2");
                    Ok(Response::new(()).into())
                },
            });
            s.route(("/", {
                let marker = marker.clone();
                move |_: &mut Input| {
                    marker.lock().unwrap().push("H1");
                    Handle::ok(Response::new(()).into())
                }
            }));
        })
        .route(("/path2", {
            let marker = marker.clone();
            move |_: &mut Input| {
                marker.lock().unwrap().push("H2");
                Handle::ok(Response::new(()).into())
            }
        }))
        .finish()
        .unwrap();

    let mut server = LocalServer::new(app).unwrap();

    let _ = server.client().get("/path1").execute().unwrap();
    assert_eq!(*marker.lock().unwrap(), vec!["B1", "B2", "H1", "A2", "A1"]);

    marker.lock().unwrap().clear();
    let _ = server.client().get("/path2").execute().unwrap();
    assert_eq!(*marker.lock().unwrap(), vec!["B1", "H2", "A1"]);
}

#[test]
fn nested_modifiers() {
    let marker = Arc::new(Mutex::new(vec![]));

    let app = App::builder()
        .mount("/path", |s| {
            s.modifier(MarkModifier {
                marker: marker.clone(),
                before: |m| {
                    m.push("B1");
                    Ok(())
                },
                after: |m| {
                    m.push("A1");
                    Ok(Response::new(()).into())
                },
            });
            s.mount("/to", |s| {
                s.modifier(MarkModifier {
                    marker: marker.clone(),
                    before: |m| {
                        m.push("B2");
                        Ok(())
                    },
                    after: |m| {
                        m.push("A2");
                        Ok(Response::new(()).into())
                    },
                });
                s.route(("/", {
                    let marker = marker.clone();
                    move |_: &mut Input| {
                        marker.lock().unwrap().push("H");
                        Handle::ok(Response::new(()).into())
                    }
                }));
            });
        })
        .finish()
        .unwrap();

    let mut server = LocalServer::new(app).unwrap();

    let _ = server.client().get("/path/to").execute().unwrap();
    assert_eq!(*marker.lock().unwrap(), vec!["B1", "B2", "H", "A2", "A1"]);
}
