#![cfg(feature = "redis-backend")]

use {
    cookie::Cookie,
    crate::{Backend, RawSession},
    futures::{try_ready, Async, Future, Poll},
    redis::{r#async::Connection, Client, RedisFuture},
    std::time::Duration,
    std::{borrow::Cow, collections::HashMap, mem},
    tsukuyomi::{
        error::{Error, Result},
        input::Input,
    },
    uuid::Uuid,
};

#[allow(missing_debug_implementations)]
#[cfg_attr(feature = "cargo-clippy", allow(stutter))]
pub struct RedisSession {
    inner: Inner,
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
}

/// A `Backend` using Redis.
#[cfg_attr(feature = "cargo-clippy", allow(stutter))]
#[derive(Debug)]
pub struct RedisBackend {
    client: Client,
    key_prefix: Cow<'static, str>,
    cookie_name: Cow<'static, str>,
    timeout: Option<Duration>,
}

impl RedisBackend {
    /// Create a new `RedisBackend` from the specified Redis client.
    pub fn new(client: Client) -> Self {
        Self {
            client,
            key_prefix: "tsukuyomi-session".into(),
            cookie_name: "session-id".into(),
            timeout: None,
        }
    }

    /// Sets the prefix of key name used at storing the session data in Redis.
    ///
    /// The default value is `"tsukuyomi-session"`.
    pub fn key_prefix(self, prefix: impl Into<Cow<'static, str>>) -> Self {
        Self {
            key_prefix: prefix.into(),
            ..self
        }
    }

    /// Sets the name of Cookie entry for storing the session ID.
    ///
    /// The default value is `"session-id"`.
    pub fn cookie_name(self, name: impl Into<Cow<'static, str>>) -> Self {
        Self {
            cookie_name: name.into(),
            ..self
        }
    }

    /// Sets the timeout to be used at storing the session data in Redis.
    ///
    /// By default, the timeout is not set.
    pub fn timeout(self, timeout: Duration) -> Self {
        Self {
            timeout: Some(timeout),
            ..self
        }
    }

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
    type ReadSession = ReadSession;
    type WriteSession = WriteSession;

    fn read(&self, input: &mut Input<'_>) -> Self::ReadSession {
        match self.get_session_id(input) {
            Ok(session_id) => {
                let key_name = session_id
                    .as_ref()
                    .map(|session_id| self.generate_redis_key(session_id));
                ReadSession::Connecting {
                    future: self.client.get_async_connection(),
                    key_name,
                    session_id,
                }
            }
            Err(err) => ReadSession::Failed(Some(err)),
        }
    }

    fn write(&self, input: &mut Input<'_>, session: Self::Session) -> Self::WriteSession {
        let RedisSession {
            inner,
            conn,
            session_id,
        } = session;

        match inner {
            Inner::Empty => WriteSession::Empty,

            Inner::Some(value) => {
                let session_id = session_id.unwrap_or_else(Uuid::new_v4);
                match input.cookies.jar() {
                    Ok(jar) => jar.add(Cookie::new(
                        self.cookie_name.clone(),
                        session_id.to_string(),
                    )),
                    Err(err) => return WriteSession::Failed(Some(err)),
                }
                let redis_key = self.generate_redis_key(&session_id);

                let value = serde_json::to_string(&value).expect("should be successed");
                let op = match self.timeout {
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
                    return WriteSession::Empty;
                };
                match input.cookies.jar() {
                    Ok(jar) => jar.remove(Cookie::named(self.cookie_name.clone())),
                    Err(err) => return WriteSession::Failed(Some(err)),
                }
                let redis_key = self.generate_redis_key(&session_id);
                let op = redis::cmd("DEL").arg(redis_key).query_async(conn);
                WriteSession::Op(op)
            }
        }
    }
}

#[allow(missing_debug_implementations)]
pub enum ReadSession {
    Failed(Option<Error>),
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

impl Future for ReadSession {
    type Item = RedisSession;
    type Error = Error;

    fn poll(&mut self) -> Poll<Self::Item, Self::Error> {
        use self::ReadSession::*;
        loop {
            let (conn, value) = match self {
                Failed(ref mut err) => {
                    return Err(err.take().expect("the future has already polled."))
                }
                Connecting { ref mut future, .. } => {
                    let conn = try_ready!(
                        future
                            .poll()
                            .map_err(tsukuyomi::error::internal_server_error)
                    );
                    (Some(conn), None)
                }
                Fetch { ref mut future, .. } => {
                    let (conn, value) = try_ready!(
                        future
                            .poll()
                            .map_err(tsukuyomi::error::internal_server_error)
                    );
                    (Some(conn), value)
                }
                Done => panic!("unexpected state"),
            };

            match (mem::replace(self, Done), conn, value) {
                (
                    Connecting {
                        key_name: Some(key_name),
                        session_id: Some(session_id),
                        ..
                    },
                    Some(conn),
                    None,
                ) => {
                    *self = Fetch {
                        future: redis::cmd("GET").arg(key_name).query_async(conn),
                        session_id,
                    };
                }

                (Fetch { session_id, .. }, Some(conn), Some(value)) => {
                    let map = serde_json::from_str(&value)
                        .map_err(tsukuyomi::error::internal_server_error)?;
                    return Ok(Async::Ready(RedisSession {
                        inner: Inner::Some(map),
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
    Empty,
    Failed(Option<Error>),
    Op(RedisFuture<(Connection, ())>),
}

impl Future for WriteSession {
    type Item = ();
    type Error = Error;

    fn poll(&mut self) -> Poll<Self::Item, Self::Error> {
        match self {
            WriteSession::Empty => Ok(Async::Ready(())),
            WriteSession::Failed(ref mut err) => {
                Err(err.take().expect("the future has already polled"))
            }
            WriteSession::Op(ref mut future) => future
                .poll()
                .map(|x| x.map(|_| ()))
                .map_err(tsukuyomi::error::internal_server_error),
        }
    }
}
