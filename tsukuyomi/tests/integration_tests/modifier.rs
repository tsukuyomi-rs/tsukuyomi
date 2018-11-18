use {
    http::Response,
    std::sync::{Arc, Mutex},
    tsukuyomi::{
        app::{route, scope, scope::Modifier},
        error::{internal_server_error, Error},
        input::Input,
        output::{Output, ResponseBody},
        AsyncResult,
    },
};

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
fn global_modifier() -> tsukuyomi::test::Result<()> {
    let marker = Arc::new(Mutex::new(vec![]));

    let mut server = tsukuyomi::app!()
        .route(
            route!() //
                .reply({
                    let marker = marker.clone();
                    move || {
                        marker.lock().unwrap().push("H");
                        ""
                    }
                }),
        ) //
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
        .build_server()?
        .into_test_server()?;

    let _ = server.perform("/")?;
    assert_eq!(*marker.lock().unwrap(), vec!["B", "H", "A"]);

    Ok(())
}

#[test]
fn global_modifier_error_on_before() -> tsukuyomi::test::Result<()> {
    let marker = Arc::new(Mutex::new(vec![]));

    let mut server = tsukuyomi::app()
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
        .build_server()?
        .into_test_server()?;

    let _ = server.perform("/")?;
    assert_eq!(*marker.lock().unwrap(), vec!["B"]);

    Ok(())
}

#[test]
fn global_modifiers() -> tsukuyomi::test::Result<()> {
    let marker = Arc::new(Mutex::new(vec![]));

    let mut server = tsukuyomi::app()
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
        .build_server()?
        .into_test_server()?;

    let _ = server.perform("/")?;
    assert_eq!(*marker.lock().unwrap(), vec!["B1", "B2", "H", "A2", "A1"]);

    Ok(())
}

#[test]
fn scoped_modifier() -> tsukuyomi::test::Result<()> {
    let marker = Arc::new(Mutex::new(vec![]));

    let mut server = tsukuyomi::app()
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
            scope!("/path1")
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
        .build_server()?
        .into_test_server()?;

    let _ = server.perform("/path1")?;
    assert_eq!(*marker.lock().unwrap(), vec!["B1", "B2", "H1", "A2", "A1"]);

    marker.lock().unwrap().clear();
    let _ = server.perform("/path2")?;
    assert_eq!(*marker.lock().unwrap(), vec!["B1", "H2", "A1"]);

    Ok(())
}

#[test]
fn nested_modifiers() -> tsukuyomi::test::Result<()> {
    let marker = Arc::new(Mutex::new(vec![]));

    let mut server = tsukuyomi::app()
        .mount(
            scope!("/path")
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
                    scope!("/to")
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
                            scope!("/a")
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
        .build_server()?
        .into_test_server()?;

    let _ = server.perform("/path/to")?;
    assert_eq!(*marker.lock().unwrap(), vec!["B1", "B2", "H1", "A2", "A1"]);

    marker.lock().unwrap().clear();
    let _ = server.perform("/path/to/a")?;
    assert_eq!(*marker.lock().unwrap(), vec!["B1", "B2", "B3", "A2", "A1"]);

    Ok(())
}
