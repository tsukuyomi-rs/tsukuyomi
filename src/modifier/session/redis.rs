#![cfg(feature = "session-redis")]

use std::borrow::Cow;
use time::Duration;
use uuid::Uuid;

use cookie::Cookie;
use redis::r#async::Connection;
use redis::Client;

use crate::error::{Failure, Result};
use crate::input::Input;
use crate::local_key;
use crate::modifier::{AfterHandle, BeforeHandle, Modifier};
use crate::output::Output;

use super::{Session, SessionState};

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
                let session_id = cookie.value().parse().map_err(Failure::bad_request)?;
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

impl Modifier for RedisSessionBackend {
    fn before_handle(&self, input: &mut Input<'_>) -> BeforeHandle {
        match self.get_session_id(input) {
            Ok(session_id) => {
                let key_name = session_id
                    .as_ref()
                    .map(|session_id| self.generate_redis_key(session_id));
                let mut read_future = self::imp::ReadFuture::connecting(
                    self.client.get_async_connection(),
                    key_name,
                    session_id,
                );
                BeforeHandle::polling(move |input| read_future.poll_session(input))
            }
            Err(err) => BeforeHandle::ready(Err(err)),
        }
    }

    fn after_handle(&self, input: &mut Input<'_>, result: Result<Output>) -> AfterHandle {
        let session = input.locals_mut().remove(&Session::KEY);
        let cx = input.locals_mut().remove(&RedisSessionContext::KEY);
        match result {
            Ok(output) => {
                let session = session.expect("should be Some");
                let cx = cx.expect("should be Some");

                let write_future = match session.state {
                    SessionState::Empty => return AfterHandle::ready(Ok(output)),
                    SessionState::Some(value) => {
                        let session_id = cx.session_id.unwrap_or_else(Uuid::new_v4);
                        match input.cookies() {
                            Ok(mut cookies) => cookies.add(Cookie::new(
                                self.cookie_name.clone(),
                                session_id.to_string(),
                            )),
                            Err(err) => return AfterHandle::ready(Err(err)),
                        }
                        let redis_key = self.generate_redis_key(&session_id);

                        let value = serde_json::to_string(&value).expect("should be successed");
                        match self.timeout {
                            Some(timeout) => Some(
                                redis::cmd("SETEX")
                                    .arg(redis_key)
                                    .arg(timeout.num_seconds())
                                    .arg(value)
                                    .query_async(cx.conn),
                            ),
                            None => Some(
                                redis::cmd("SET")
                                    .arg(redis_key)
                                    .arg(value)
                                    .query_async(cx.conn),
                            ),
                        }
                    }
                    SessionState::Clear => {
                        if let Some(session_id) = cx.session_id {
                            match input.cookies() {
                                Ok(mut cookies) => {
                                    cookies.remove(Cookie::named(self.cookie_name.clone()))
                                }
                                Err(err) => return AfterHandle::ready(Err(err)),
                            }
                            let redis_key = self.generate_redis_key(&session_id);
                            Some(redis::cmd("DEL").arg(redis_key).query_async(cx.conn))
                        } else {
                            None
                        }
                    }
                };

                match write_future {
                    Some(mut write_future) => {
                        let mut output_opt = Some(output);
                        AfterHandle::polling(move |_input| {
                            let (_conn, ()) = futures::try_ready!(
                                write_future.poll().map_err(Failure::internal_server_error)
                            );
                            let output = output_opt.take().unwrap();
                            Ok(output.into())
                        })
                    }
                    None => AfterHandle::ready(Ok(output)),
                }
            }
            Err(err) => AfterHandle::ready(Err(err)),
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
    local_key!(const KEY: Self);
}

mod imp {
    use std::mem;

    use futures::{try_ready, Future, Poll};
    use redis::r#async::Connection;
    use redis::RedisFuture;
    use uuid::Uuid;

    use crate::error::{Error, Failure};
    use crate::input::Input;
    use crate::output::Output;

    use super::super::Session;
    use super::RedisSessionContext;

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

        pub fn poll_session(&mut self, input: &mut Input<'_>) -> Poll<Option<Output>, Error> {
            use self::ReadFutureState::*;
            loop {
                let (conn, value) = match self.state {
                    Connecting { ref mut future, .. } => {
                        let conn =
                            try_ready!(future.poll().map_err(Failure::internal_server_error));
                        (Some(conn), None)
                    }
                    Fetch { ref mut future, .. } => {
                        let (conn, value) =
                            try_ready!(future.poll().map_err(Failure::internal_server_error));
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
                        let map =
                            serde_json::from_str(&value).map_err(Failure::internal_server_error)?;
                        input.locals_mut().insert(&Session::KEY, Session::some(map));
                        input.locals_mut().insert(
                            &RedisSessionContext::KEY,
                            RedisSessionContext {
                                conn,
                                session_id: Some(session_id),
                            },
                        );
                        return Ok(None.into());
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
                        input.locals_mut().insert(&Session::KEY, Session::empty());
                        input.locals_mut().insert(
                            &RedisSessionContext::KEY,
                            RedisSessionContext {
                                conn,
                                session_id: None,
                            },
                        );
                        return Ok(None.into());
                    }

                    _ => unreachable!("unexpected condition"),
                }
            }
        }
    }
}
