#![allow(missing_docs)]
#![cfg(feature = "redis-backend")]

use {
    super::imp::{Backend, BackendImpl},
    cookie::Cookie,
    crate::session::SessionInner,
    futures::{try_ready, Async, Future, Poll},
    redis::{r#async::Connection, Client, RedisFuture},
    std::{borrow::Cow, mem},
    time::Duration,
    tsukuyomi::{
        error::{Error, Result},
        input::{local_map::local_key, Input},
    },
    uuid::Uuid,
};

#[cfg_attr(feature = "cargo-clippy", allow(stutter))]
#[derive(Debug)]
pub struct RedisSessionBackend {
    client: Client,
    key_prefix: Cow<'static, str>,
    cookie_name: Cow<'static, str>,
    timeout: Option<Duration>,
}

impl RedisSessionBackend {
    pub fn new(client: Client) -> Self {
        Self {
            client,
            key_prefix: "tsukuyomi-session".into(),
            cookie_name: "session-id".into(),
            timeout: None,
        }
    }

    fn generate_redis_key(&self, id: &Uuid) -> String {
        format!("{}:{}", self.key_prefix, id)
    }

    fn get_session_id(&self, input: &mut Input<'_>) -> Result<Option<Uuid>> {
        let cookies = input.cookies()?;
        match cookies.get(&self.cookie_name) {
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

    pub fn key_prefix(self, prefix: impl Into<Cow<'static, str>>) -> Self {
        Self {
            key_prefix: prefix.into(),
            ..self
        }
    }

    pub fn cookie_name(self, name: impl Into<Cow<'static, str>>) -> Self {
        Self {
            cookie_name: name.into(),
            ..self
        }
    }

    pub fn timeout(self, timeout: Duration) -> Self {
        Self {
            timeout: Some(timeout),
            ..self
        }
    }
}

impl Backend for RedisSessionBackend {}
impl BackendImpl for RedisSessionBackend {
    type ReadFuture = ReadFuture;
    type WriteFuture = WriteFuture;

    fn read(&self, input: &mut Input<'_>) -> Self::ReadFuture {
        match self.get_session_id(input) {
            Ok(session_id) => {
                let key_name = session_id
                    .as_ref()
                    .map(|session_id| self.generate_redis_key(session_id));

                ReadFuture::connecting(self.client.get_async_connection(), key_name, session_id)
            }
            Err(err) => ReadFuture::err(err),
        }
    }

    fn write(&self, input: &mut Input<'_>, state: SessionInner) -> Self::WriteFuture {
        let RedisSessionContext { conn, session_id } = input
            .locals_mut()
            .remove(&RedisSessionContext::KEY)
            .expect("should be Some");

        match state {
            SessionInner::Empty => WriteFuture::no_op(),

            SessionInner::Some(value) => {
                let session_id = session_id.unwrap_or_else(Uuid::new_v4);
                match input.cookies() {
                    Ok(mut cookies) => cookies.add(Cookie::new(
                        self.cookie_name.clone(),
                        session_id.to_string(),
                    )),
                    Err(err) => return WriteFuture::err(err),
                }
                let redis_key = self.generate_redis_key(&session_id);

                let value = serde_json::to_string(&value).expect("should be successed");
                match self.timeout {
                    Some(timeout) => WriteFuture::op(
                        redis::cmd("SETEX")
                            .arg(redis_key)
                            .arg(timeout.num_seconds())
                            .arg(value)
                            .query_async(conn),
                    ),
                    None => WriteFuture::op(
                        redis::cmd("SET")
                            .arg(redis_key)
                            .arg(value)
                            .query_async(conn),
                    ),
                }
            }

            SessionInner::Clear => {
                if let Some(session_id) = session_id {
                    match input.cookies() {
                        Ok(mut cookies) => cookies.remove(Cookie::named(self.cookie_name.clone())),
                        Err(err) => return WriteFuture::err(err),
                    }
                    let redis_key = self.generate_redis_key(&session_id);
                    WriteFuture::op(redis::cmd("DEL").arg(redis_key).query_async(conn))
                } else {
                    WriteFuture::no_op()
                }
            }
        }
    }
}

#[cfg_attr(feature = "cargo-clippy", allow(stutter))]
#[allow(missing_debug_implementations)]
struct RedisSessionContext {
    conn: Connection,
    session_id: Option<Uuid>,
}

impl RedisSessionContext {
    local_key! {
        const KEY: Self;
    }
}

#[allow(missing_debug_implementations)]
pub struct ReadFuture {
    state: ReadFutureState,
}

#[allow(missing_debug_implementations)]
enum ReadFutureState {
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

impl ReadFuture {
    pub(super) fn err(err: Error) -> Self {
        Self {
            state: ReadFutureState::Failed(Some(err)),
        }
    }

    pub(super) fn connecting(
        future: RedisFuture<Connection>,
        key_name: Option<String>,
        session_id: Option<Uuid>,
    ) -> Self {
        Self {
            state: ReadFutureState::Connecting {
                future,
                key_name,
                session_id,
            },
        }
    }
}

impl super::imp::ReadFuture for ReadFuture {
    fn poll_read(&mut self, input: &mut Input<'_>) -> Poll<SessionInner, Error> {
        use self::ReadFutureState::*;
        loop {
            let (conn, value) = match self.state {
                Failed(ref mut err) => return Err(err.take().unwrap()),
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
                    input.locals_mut().insert(
                        &RedisSessionContext::KEY,
                        RedisSessionContext {
                            conn,
                            session_id: Some(session_id),
                        },
                    );
                    let map = serde_json::from_str(&value)
                        .map_err(tsukuyomi::error::internal_server_error)?;
                    let state = SessionInner::Some(map);
                    return Ok(Async::Ready(state));
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
                    input.locals_mut().insert(
                        &RedisSessionContext::KEY,
                        RedisSessionContext {
                            conn,
                            session_id: None,
                        },
                    );
                    return Ok(Async::Ready(SessionInner::Empty));
                }

                _ => unreachable!("unexpected condition"),
            }
        }
    }
}

#[allow(missing_debug_implementations)]
pub struct WriteFuture {
    state: WriteFutureState,
}

enum WriteFutureState {
    NoOp,
    Failed(Option<Error>),
    Op(RedisFuture<(Connection, ())>),
}

impl WriteFuture {
    pub(super) fn no_op() -> Self {
        Self {
            state: WriteFutureState::NoOp,
        }
    }

    pub(super) fn err(err: Error) -> Self {
        Self {
            state: WriteFutureState::Failed(Some(err)),
        }
    }

    pub(super) fn op(future: RedisFuture<(Connection, ())>) -> Self {
        Self {
            state: WriteFutureState::Op(future),
        }
    }
}

impl super::imp::WriteFuture for WriteFuture {
    fn poll_write(&mut self, _: &mut Input<'_>) -> Poll<(), Error> {
        use self::WriteFutureState::*;
        match self.state {
            NoOp => Ok(Async::Ready(())),
            Failed(ref mut err) => Err(err.take().unwrap()),
            Op(ref mut future) => future
                .poll()
                .map(|x| x.map(|_| ()))
                .map_err(tsukuyomi::error::internal_server_error),
        }
    }
}
