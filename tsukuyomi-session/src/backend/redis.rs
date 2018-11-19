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
        handler::AsyncResult,
        input::Input,
        localmap::local_key,
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
    fn read(&self) -> AsyncResult<SessionInner> {
        let mut read_future: Option<ReadFuture> = None;
        AsyncResult::poll_fn(move |input| {
            let this = input
                .state_detached::<Self>()
                .expect("the backend is not set");
            let this = this.get(input);

            loop {
                if let Some(ref mut read_future) = read_future {
                    return read_future.poll_ready(input);
                }

                let session_id = this.get_session_id(input)?;
                let key_name = session_id
                    .as_ref()
                    .map(|session_id| this.generate_redis_key(session_id));
                read_future = Some(ReadFuture::connecting(
                    this.client.get_async_connection(),
                    key_name,
                    session_id,
                ));
            }
        })
    }

    fn write(&self, inner: SessionInner) -> AsyncResult<()> {
        let mut inner = Some(inner);
        let mut future: Option<RedisFuture<(_, ())>> = None;

        AsyncResult::poll_fn(move |input| {
            let this = input.state_detached::<Self>().expect("backend is not set");
            let this = this.get(input);

            loop {
                if let Some(ref mut future) = future {
                    return future
                        .poll()
                        .map(|x| x.map(|_| ()))
                        .map_err(tsukuyomi::error::internal_server_error);
                }

                let RedisSessionContext { conn, session_id } = input
                    .locals_mut()
                    .remove(&RedisSessionContext::KEY)
                    .expect("should be Some");

                let op = match inner.take().expect("should be available") {
                    SessionInner::Empty => return Ok(Async::Ready(())),

                    SessionInner::Some(value) => {
                        let session_id = session_id.unwrap_or_else(Uuid::new_v4);
                        let mut cookies = input.cookies()?;
                        cookies.add(Cookie::new(
                            this.cookie_name.clone(),
                            session_id.to_string(),
                        ));
                        let redis_key = this.generate_redis_key(&session_id);

                        let value = serde_json::to_string(&value).expect("should be successed");
                        match this.timeout {
                            Some(timeout) => redis::cmd("SETEX")
                                .arg(redis_key)
                                .arg(timeout.num_seconds())
                                .arg(value)
                                .query_async(conn),
                            None => redis::cmd("SET")
                                .arg(redis_key)
                                .arg(value)
                                .query_async(conn),
                        }
                    }

                    SessionInner::Clear => {
                        let session_id = if let Some(session_id) = session_id {
                            session_id
                        } else {
                            return Ok(Async::Ready(()));
                        };
                        let mut cookies = input.cookies()?;
                        cookies.remove(Cookie::named(this.cookie_name.clone()));
                        let redis_key = this.generate_redis_key(&session_id);
                        redis::cmd("DEL").arg(redis_key).query_async(conn)
                    }
                };
                future = Some(op);
            }
        })
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
    fn connecting(
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

    fn poll_ready(&mut self, input: &mut Input<'_>) -> Poll<SessionInner, Error> {
        use self::ReadFutureState::*;
        loop {
            let (conn, value) = match self.state {
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
