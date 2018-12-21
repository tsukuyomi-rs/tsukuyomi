#![cfg(feature = "use-redis")]

use {
    crate::{Backend, RawSession},
    cookie::Cookie,
    futures::try_ready,
    redis::{r#async::Connection, Client, RedisFuture},
    std::time::Duration,
    std::{borrow::Cow, collections::HashMap, mem, sync::Arc},
    tsukuyomi::{
        error::{Error, Result},
        future::{Async, Poll, TryFuture},
        input::Input,
    },
    uuid::Uuid,
};

/// A `Backend` using Redis.
#[derive(Debug, Clone)]
pub struct RedisBackend {
    inner: Arc<RedisBackendInner>,
}

impl RedisBackend {
    /// Create a new `RedisBackend` from the specified Redis client.
    pub fn new(client: Client) -> Self {
        Self {
            inner: Arc::new(RedisBackendInner {
                client,
                key_prefix: "tsukuyomi-session".into(),
                cookie_name: "session-id".into(),
                timeout: None,
            }),
        }
    }

    fn inner_mut(&mut self) -> &mut RedisBackendInner {
        Arc::get_mut(&mut self.inner).expect("the value has already been shared")
    }

    /// Sets the prefix of key name used at storing the session data in Redis.
    ///
    /// The default value is `"tsukuyomi-session"`.
    pub fn key_prefix(mut self, prefix: impl Into<Cow<'static, str>>) -> Self {
        self.inner_mut().key_prefix = prefix.into();
        self
    }

    /// Sets the name of Cookie entry for storing the session ID.
    ///
    /// The default value is `"session-id"`.
    pub fn cookie_name(mut self, name: impl Into<Cow<'static, str>>) -> Self {
        self.inner_mut().cookie_name = name.into();
        self
    }

    /// Sets the timeout to be used at storing the session data in Redis.
    ///
    /// By default, the timeout is not set.
    pub fn timeout(mut self, timeout: Duration) -> Self {
        self.inner_mut().timeout = Some(timeout);
        self
    }
}

#[derive(Debug)]
struct RedisBackendInner {
    client: Client,
    key_prefix: Cow<'static, str>,
    cookie_name: Cow<'static, str>,
    timeout: Option<Duration>,
}

impl RedisBackendInner {
    fn generate_redis_key(&self, id: &Uuid) -> String {
        format!("{}:{}", self.key_prefix, id)
    }

    fn get_session_id(&self, input: &mut Input<'_>) -> Result<Option<Uuid>> {
        match input.cookies.jar()?.get(&self.cookie_name) {
            Some(cookie) => {
                let session_id = cookie
                    .value()
                    .parse()
                    .map_err(tsukuyomi::error::bad_request)?;
                Ok(Some(session_id))
            }
            None => Ok(None),
        }
    }
}

impl Backend for RedisBackend {
    type Session = RedisSession;
    type ReadError = Error;
    type ReadSession = ReadSession;

    fn read(&self) -> Self::ReadSession {
        ReadSession {
            state: ReadSessionState::Init,
            backend: Some(self.clone()),
        }
    }
}

#[allow(missing_debug_implementations)]
pub struct RedisSession {
    inner: Inner,
    backend: RedisBackend,
    conn: Connection,
    session_id: Option<Uuid>,
}

#[derive(Debug)]
enum Inner {
    Empty,
    Some(HashMap<String, String>),
    Clear,
}

impl RawSession for RedisSession {
    type WriteError = Error;
    type WriteSession = WriteSession;

    fn get(&self, name: &str) -> Option<&str> {
        match self.inner {
            Inner::Some(ref map) => map.get(name).map(|s| &**s),
            _ => None,
        }
    }

    fn set(&mut self, name: &str, value: String) {
        match self.inner {
            Inner::Empty => {}
            Inner::Some(ref mut map) => {
                map.insert(name.to_owned(), value);
                return;
            }
            Inner::Clear => return,
        }

        match std::mem::replace(&mut self.inner, Inner::Empty) {
            Inner::Empty => {
                self.inner = Inner::Some({
                    let mut map = HashMap::new();
                    map.insert(name.to_owned(), value);
                    map
                });
            }
            Inner::Some(..) | Inner::Clear => unreachable!(),
        }
    }

    fn remove(&mut self, name: &str) {
        if let Inner::Some(ref mut map) = self.inner {
            map.remove(name);
        }
    }

    fn clear(&mut self) {
        self.inner = Inner::Clear;
    }

    fn write(self) -> Self::WriteSession {
        WriteSession::Init(Some(self))
    }
}

#[allow(missing_debug_implementations)]
pub struct ReadSession {
    backend: Option<RedisBackend>,
    state: ReadSessionState,
}

enum ReadSessionState {
    Init,
    Connecting {
        future: RedisFuture<Connection>,
        key_name: Option<String>,
        session_id: Option<Uuid>,
    },
    Fetch {
        future: RedisFuture<(Connection, Option<String>)>,
        session_id: Uuid,
    },
    Done,
}

impl TryFuture for ReadSession {
    type Ok = RedisSession;
    type Error = Error;

    fn poll_ready(&mut self, input: &mut Input<'_>) -> Poll<Self::Ok, Self::Error> {
        use self::ReadSessionState::*;
        loop {
            let (conn, value) = match self.state {
                Init => {
                    let backend = self.backend.as_ref().expect("unexpected condition");
                    let session_id = backend.inner.get_session_id(input)?;
                    let key_name = session_id
                        .as_ref()
                        .map(|session_id| backend.inner.generate_redis_key(session_id));
                    self.state = ReadSessionState::Connecting {
                        future: backend.inner.client.get_async_connection(),
                        key_name,
                        session_id,
                    };
                    continue;
                }
                Connecting { ref mut future, .. } => {
                    let conn = try_ready!(future
                        .poll()
                        .map_err(tsukuyomi::error::internal_server_error));
                    (Some(conn), None)
                }
                Fetch { ref mut future, .. } => {
                    let (conn, value) = try_ready!(future
                        .poll()
                        .map_err(tsukuyomi::error::internal_server_error));
                    (Some(conn), value)
                }
                Done => panic!("unexpected state"),
            };

            match (mem::replace(&mut self.state, Done), conn, value) {
                (
                    Connecting {
                        key_name: Some(key_name),
                        session_id: Some(session_id),
                        ..
                    },
                    Some(conn),
                    None,
                ) => {
                    self.state = Fetch {
                        future: redis::cmd("GET").arg(key_name).query_async(conn),
                        session_id,
                    };
                }

                (Fetch { session_id, .. }, Some(conn), Some(value)) => {
                    let map = serde_json::from_str(&value)
                        .map_err(tsukuyomi::error::internal_server_error)?;
                    return Ok(Async::Ready(RedisSession {
                        inner: Inner::Some(map),
                        backend: self
                            .backend
                            .take()
                            .expect("the future has already been polled."),
                        conn,
                        session_id: Some(session_id),
                    }));
                }

                (
                    Connecting {
                        session_id: None,
                        key_name: None,
                        ..
                    },
                    Some(conn),
                    None,
                )
                | (Fetch { .. }, Some(conn), None) => {
                    return Ok(Async::Ready(RedisSession {
                        inner: Inner::Empty,
                        backend: self
                            .backend
                            .take()
                            .expect("the future has already been polled."),
                        conn,
                        session_id: None,
                    }));
                }

                _ => unreachable!("unexpected condition"),
            }
        }
    }
}

#[allow(missing_debug_implementations)]
pub enum WriteSession {
    Init(Option<RedisSession>),
    Op(RedisFuture<(Connection, ())>),
}

impl TryFuture for WriteSession {
    type Ok = ();
    type Error = Error;

    fn poll_ready(&mut self, input: &mut Input<'_>) -> Poll<Self::Ok, Self::Error> {
        loop {
            *self = match self {
                WriteSession::Init(ref mut session) => {
                    let RedisSession {
                        inner,
                        backend,
                        conn,
                        session_id,
                    } = session.take().unwrap();

                    match inner {
                        Inner::Empty => return Ok(Async::Ready(())),

                        Inner::Some(value) => {
                            let session_id = session_id.unwrap_or_else(Uuid::new_v4);
                            match input.cookies.jar() {
                                Ok(jar) => jar.add(Cookie::new(
                                    backend.inner.cookie_name.clone(),
                                    session_id.to_string(),
                                )),
                                Err(err) => return Err(err),
                            }
                            let redis_key = backend.inner.generate_redis_key(&session_id);

                            let value = serde_json::to_string(&value).expect("should be successed");
                            let op = match backend.inner.timeout {
                                Some(timeout) => redis::cmd("SETEX")
                                    .arg(redis_key)
                                    .arg(timeout.as_secs())
                                    .arg(value)
                                    .query_async(conn),
                                None => redis::cmd("SET")
                                    .arg(redis_key)
                                    .arg(value)
                                    .query_async(conn),
                            };
                            WriteSession::Op(op)
                        }

                        Inner::Clear => {
                            let session_id = if let Some(session_id) = session_id {
                                session_id
                            } else {
                                return Ok(Async::Ready(()));
                            };
                            match input.cookies.jar() {
                                Ok(jar) => {
                                    jar.remove(Cookie::named(backend.inner.cookie_name.clone()))
                                }
                                Err(err) => return Err(err),
                            }
                            let redis_key = backend.inner.generate_redis_key(&session_id);
                            let op = redis::cmd("DEL").arg(redis_key).query_async(conn);
                            WriteSession::Op(op)
                        }
                    }
                }
                WriteSession::Op(ref mut op) => {
                    return op
                        .poll()
                        .map(|x| x.map(|_| ()))
                        .map_err(tsukuyomi::error::internal_server_error)
                }
            }
        }
    }
}
