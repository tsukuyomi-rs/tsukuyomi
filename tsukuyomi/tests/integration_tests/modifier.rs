use tsukuyomi::app::{scope, App};
use tsukuyomi::app::{AsyncResult, Modifier};
use tsukuyomi::error::internal_server_error;
use tsukuyomi::error::Error;
use tsukuyomi::input::Input;
use tsukuyomi::output::{Output, ResponseBody};
use tsukuyomi::route;
use tsukuyomi::test::test_server;

use http::{Request, Response};
use std::sync::{Arc, Mutex};

struct MarkModifier<T1, T2>
where
    T1: Fn(&mut Vec<&'static str>) -> Result<Option<Output>, Error>,
    T2: Fn(&mut Vec<&'static str>) -> Result<Output, Error>,
{
    marker: Arc<Mutex<Vec<&'static str>>>,
    before: T1,
    after: T2,
}

impl<T1, T2> Modifier for MarkModifier<T1, T2>
where
    T1: Fn(&mut Vec<&'static str>) -> Result<Option<Output>, Error>,
    T2: Fn(&mut Vec<&'static str>) -> Result<Output, Error>,
{
    fn before_handle(&self, _: &mut Input) -> AsyncResult<Option<Output>> {
        (self.before)(&mut *self.marker.lock().unwrap()).into()
    }

    fn after_handle(&self, _: &mut Input, _: Result<Output, Error>) -> AsyncResult<Output> {
        (self.after)(&mut *self.marker.lock().unwrap()).into()
    }
}

#[test]
fn global_modifier() {
    let marker = Arc::new(Mutex::new(vec![]));

    let mut server = test_server(
        App::builder()
            .route(route!().reply({
                let marker = marker.clone();
                move || {
                    marker.lock().unwrap().push("H");
                    ""
                }
            })) //
            .modifier(MarkModifier {
                marker: marker.clone(),
                before: |m| {
                    m.push("B");
                    Ok(None)
                },
                after: |m| {
                    m.push("A");
                    Ok(Response::new(ResponseBody::empty()))
                },
            }) //
            .finish()
            .unwrap(),
    );

    let _ = server.perform(Request::get("/")).unwrap();
    assert_eq!(*marker.lock().unwrap(), vec!["B", "H", "A"]);
}

#[test]
fn global_modifier_error_on_before() {
    let marker = Arc::new(Mutex::new(vec![]));

    let mut server = test_server(
        App::builder()
            .route(route!().reply({
                let marker = marker.clone();
                move || {
                    marker.lock().unwrap().push("H");
                    ""
                }
            })) //
            .modifier(MarkModifier {
                marker: marker.clone(),
                before: |m| {
                    m.push("B");
                    Err(internal_server_error(""))
                },
                after: |m| {
                    m.push("A");
                    Ok(Response::new(ResponseBody::empty()))
                },
            }) //
            .finish()
            .unwrap(),
    );

    let _ = server.perform(Request::get("/")).unwrap();
    assert_eq!(*marker.lock().unwrap(), vec!["B"]);
}

#[test]
fn global_modifiers() {
    let marker = Arc::new(Mutex::new(vec![]));

    let mut server = test_server(
        App::builder()
            .route(route!().reply({
                let marker = marker.clone();
                move || {
                    marker.lock().unwrap().push("H");
                    ""
                }
            })) //
            .modifier(MarkModifier {
                marker: marker.clone(),
                before: |m| {
                    m.push("B1");
                    Ok(None)
                },
                after: |m| {
                    m.push("A1");
                    Ok(Response::new(ResponseBody::empty()))
                },
            }) //
            .modifier(MarkModifier {
                marker: marker.clone(),
                before: |m| {
                    m.push("B2");
                    Ok(None)
                },
                after: |m| {
                    m.push("A2");
                    Ok(Response::new(ResponseBody::empty()))
                },
            }) //
            .finish()
            .unwrap(),
    );

    let _ = server.perform(Request::get("/")).unwrap();
    assert_eq!(*marker.lock().unwrap(), vec!["B1", "B2", "H", "A2", "A1"]);
}

#[test]
fn scoped_modifier() {
    let marker = Arc::new(Mutex::new(vec![]));

    let mut server = test_server(
        App::builder()
            .modifier(MarkModifier {
                marker: marker.clone(),
                before: |m| {
                    m.push("B1");
                    Ok(None)
                },
                after: |m| {
                    m.push("A1");
                    Ok(Response::new(ResponseBody::empty()))
                },
            }) //
            .mount(
                scope::with_prefix("/path1")
                    .modifier(MarkModifier {
                        marker: marker.clone(),
                        before: |m| {
                            m.push("B2");
                            Ok(None)
                        },
                        after: |m| {
                            m.push("A2");
                            Ok(Response::new(ResponseBody::empty()))
                        },
                    }) //
                    .route(route!().reply({
                        let marker = marker.clone();
                        move || {
                            marker.lock().unwrap().push("H1");
                            ""
                        }
                    })), //
            ) //
            .route(route!("/path2").reply({
                let marker = marker.clone();
                move || {
                    marker.lock().unwrap().push("H2");
                    ""
                }
            })) //
            .finish()
            .unwrap(),
    );

    let _ = server.perform(Request::get("/path1")).unwrap();
    assert_eq!(*marker.lock().unwrap(), vec!["B1", "B2", "H1", "A2", "A1"]);

    marker.lock().unwrap().clear();
    let _ = server.perform(Request::get("/path2")).unwrap();
    assert_eq!(*marker.lock().unwrap(), vec!["B1", "H2", "A1"]);
}

#[test]
fn nested_modifiers() {
    let marker = Arc::new(Mutex::new(vec![]));

    let mut server = test_server(
        App::builder()
            .mount(
                scope::with_prefix("/path")
                    .modifier(MarkModifier {
                        marker: marker.clone(),
                        before: |m| {
                            m.push("B1");
                            Ok(None)
                        },
                        after: |m| {
                            m.push("A1");
                            Ok(Response::new(ResponseBody::empty()))
                        },
                    }) //
                    .mount(
                        scope::with_prefix("/to")
                            .modifier(MarkModifier {
                                marker: marker.clone(),
                                before: |m| {
                                    m.push("B2");
                                    Ok(None)
                                },
                                after: |m| {
                                    m.push("A2");
                                    Ok(Response::new(ResponseBody::empty()))
                                },
                            }) //
                            .route(route!().reply({
                                let marker = marker.clone();
                                move || {
                                    marker.lock().unwrap().push("H1");
                                    ""
                                }
                            })) //
                            .mount(
                                scope::with_prefix("/a")
                                    .modifier(MarkModifier {
                                        marker: marker.clone(),
                                        before: |m| {
                                            m.push("B3");
                                            Ok(Some(Response::new(ResponseBody::empty())))
                                        },
                                        after: |m| {
                                            m.push("A3");
                                            Ok(Response::new(ResponseBody::empty()))
                                        },
                                    }) //
                                    .route(route!().reply({
                                        let marker = marker.clone();
                                        move || {
                                            marker.lock().unwrap().push("H2");
                                            ""
                                        }
                                    })),
                            ),
                    ),
            ) //
            .finish()
            .unwrap(),
    );

    let _ = server.perform(Request::get("/path/to")).unwrap();
    assert_eq!(*marker.lock().unwrap(), vec!["B1", "B2", "H1", "A2", "A1"]);

    marker.lock().unwrap().clear();
    let _ = server.perform(Request::get("/path/to/a")).unwrap();
    assert_eq!(*marker.lock().unwrap(), vec!["B1", "B2", "B3", "A2", "A1"]);
}
