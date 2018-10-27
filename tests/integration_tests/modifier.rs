use tsukuyomi::app::builder::Route;
use tsukuyomi::app::App;
use tsukuyomi::error::internal_server_error;
use tsukuyomi::error::Error;
use tsukuyomi::input::Input;
use tsukuyomi::modifier::{AfterHandle, BeforeHandle, Modifier};
use tsukuyomi::output::{Output, ResponseBody};

use http::{Request, Response};
use std::sync::{Arc, Mutex};

use super::util::{local_server, wrap_ready, LocalServerExt};

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
    fn before_handle(&self, _: &mut Input) -> BeforeHandle {
        (self.before)(&mut *self.marker.lock().unwrap()).into()
    }

    fn after_handle(&self, _: &mut Input, _: Result<Output, Error>) -> AfterHandle {
        (self.after)(&mut *self.marker.lock().unwrap()).into()
    }
}

#[test]
fn global_modifier() {
    let marker = Arc::new(Mutex::new(vec![]));

    let mut server = local_server(
        App::builder()
            .route(("/", {
                let marker = marker.clone();
                wrap_ready(move |_| {
                    marker.lock().unwrap().push("H");
                    ""
                })
            })).modifier(MarkModifier {
                marker: marker.clone(),
                before: |m| {
                    m.push("B");
                    Ok(None)
                },
                after: |m| {
                    m.push("A");
                    Ok(Response::new(ResponseBody::empty()))
                },
            }),
    );

    let _ = server.perform(Request::get("/")).unwrap();
    assert_eq!(*marker.lock().unwrap(), vec!["B", "H", "A"]);
}

#[test]
fn global_modifier_error_on_before() {
    let marker = Arc::new(Mutex::new(vec![]));

    let mut server = local_server(
        App::builder()
            .route(("/", {
                let marker = marker.clone();
                wrap_ready(move |_| {
                    marker.lock().unwrap().push("H");
                    ""
                })
            })).modifier(MarkModifier {
                marker: marker.clone(),
                before: |m| {
                    m.push("B");
                    Err(internal_server_error("").into())
                },
                after: |m| {
                    m.push("A");
                    Ok(Response::new(ResponseBody::empty()))
                },
            }),
    );

    let _ = server.perform(Request::get("/")).unwrap();
    assert_eq!(*marker.lock().unwrap(), vec!["B"]);
}

#[test]
fn global_modifiers() {
    let marker = Arc::new(Mutex::new(vec![]));

    let mut server = local_server(
        App::builder()
            .route(("/", {
                let marker = marker.clone();
                wrap_ready(move |_| {
                    marker.lock().unwrap().push("H");
                    ""
                })
            })).modifier(MarkModifier {
                marker: marker.clone(),
                before: |m| {
                    m.push("B1");
                    Ok(None)
                },
                after: |m| {
                    m.push("A1");
                    Ok(Response::new(ResponseBody::empty()))
                },
            }).modifier(MarkModifier {
                marker: marker.clone(),
                before: |m| {
                    m.push("B2");
                    Ok(None)
                },
                after: |m| {
                    m.push("A2");
                    Ok(Response::new(ResponseBody::empty()))
                },
            }),
    );

    let _ = server.perform(Request::get("/")).unwrap();
    assert_eq!(*marker.lock().unwrap(), vec!["B1", "B2", "H", "A2", "A1"]);
}

#[test]
fn scoped_modifier() {
    let marker = Arc::new(Mutex::new(vec![]));

    let mut server = local_server(
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
            }).mount("/path1", |s| {
                s.modifier(MarkModifier {
                    marker: marker.clone(),
                    before: |m| {
                        m.push("B2");
                        Ok(None)
                    },
                    after: |m| {
                        m.push("A2");
                        Ok(Response::new(ResponseBody::empty()))
                    },
                });
                s.route(("/", {
                    let marker = marker.clone();
                    wrap_ready(move |_| {
                        marker.lock().unwrap().push("H1");
                        ""
                    })
                }));
            }).route(("/path2", {
                let marker = marker.clone();
                wrap_ready(move |_| {
                    marker.lock().unwrap().push("H2");
                    ""
                })
            })),
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

    let mut server = local_server(App::builder().mount("/path", |s| {
        s.modifier(MarkModifier {
            marker: marker.clone(),
            before: |m| {
                m.push("B1");
                Ok(None)
            },
            after: |m| {
                m.push("A1");
                Ok(Response::new(ResponseBody::empty()))
            },
        });
        s.mount("/to", |s| {
            s.modifier(MarkModifier {
                marker: marker.clone(),
                before: |m| {
                    m.push("B2");
                    Ok(None)
                },
                after: |m| {
                    m.push("A2");
                    Ok(Response::new(ResponseBody::empty()))
                },
            });
            s.route(("/", {
                let marker = marker.clone();
                wrap_ready(move |_| {
                    marker.lock().unwrap().push("H1");
                    ""
                })
            }));

            s.mount("/a", |s| {
                s.modifier(MarkModifier {
                    marker: marker.clone(),
                    before: |m| {
                        m.push("B3");
                        Ok(Some(Response::new(ResponseBody::empty())))
                    },
                    after: |m| {
                        m.push("A3");
                        Ok(Response::new(ResponseBody::empty()))
                    },
                });
                s.route(("/", {
                    let marker = marker.clone();
                    wrap_ready(move |_| {
                        marker.lock().unwrap().push("H2");
                        ""
                    })
                }));
            });
        });
    }));

    let _ = server.perform(Request::get("/path/to")).unwrap();
    assert_eq!(*marker.lock().unwrap(), vec!["B1", "B2", "H1", "A2", "A1"]);

    marker.lock().unwrap().clear();
    let _ = server.perform(Request::get("/path/to/a")).unwrap();
    assert_eq!(*marker.lock().unwrap(), vec!["B1", "B2", "B3", "A2", "A1"]);
}

#[test]
fn route_modifiers() {
    let marker = Arc::new(Mutex::new(vec![]));

    let mut server = local_server(App::builder().mount("/path", |s| {
        s.modifier(MarkModifier {
            marker: marker.clone(),
            before: |m| {
                m.push("B1");
                Ok(None)
            },
            after: |m| {
                m.push("A1");
                Ok(Response::new(ResponseBody::empty()))
            },
        });
        s.mount("/to", |s| {
            s.route(|r: &mut Route| {
                r.uri("/");
                r.modifier(MarkModifier {
                    marker: marker.clone(),
                    before: |m| {
                        m.push("B2");
                        Ok(None)
                    },
                    after: |m| {
                        m.push("A2");
                        Ok(Response::new(ResponseBody::empty()))
                    },
                });

                r.handler({
                    let marker = marker.clone();
                    wrap_ready(move |_| {
                        marker.lock().unwrap().push("H");
                        ""
                    })
                });
            });
        });
    }));

    let _ = server.perform(Request::get("/path/to")).unwrap();
    assert_eq!(*marker.lock().unwrap(), vec!["B1", "B2", "H", "A2", "A1"]);
}
