//! Experimental testing utility.

#![allow(missing_docs)]
#![allow(missing_debug_implementations)]
#![allow(clippy::unimplemented)]

use {
    crate::{
        app::{config::ThreadSafe, App, AppService},
        input::body::RequestBody,
        output::body::ResponseBody,
    },
    bytes::Bytes,
    http::{Request, Response},
    izanami::http::{body::HttpBodyExt, HttpService},
    std::fmt,
    tokio::runtime::Runtime,
    tokio_buf::BufStreamExt,
};

pub use crate::loc;

#[derive(Debug)]
pub struct Location {
    #[doc(hidden)]
    pub file: &'static str,
    #[doc(hidden)]
    pub line: u32,
}

#[macro_export]
macro_rules! loc {
    () => {
        $crate::test::Location {
            file: file!(),
            line: line!(),
        }
    };
}

pub type Result<T = ()> = std::result::Result<T, exitfailure::ExitFailure>;

pub struct TestServer {
    app: App,
    runtime: Runtime,
}

impl TestServer {
    pub fn new(app: App) -> Result<Self> {
        Ok(Self {
            app,
            runtime: {
                let mut builder = tokio::runtime::Builder::new();
                builder.core_threads(1);
                builder.blocking_threads(1);
                builder.build()?
            },
        })
    }

    pub fn connect(&mut self) -> TestClient<'_> {
        TestClient {
            service: self.app.new_service(),
            server: self,
        }
    }
}

pub struct TestClient<'a> {
    service: AppService<ThreadSafe>,
    server: &'a mut TestServer,
}

impl<'a> TestClient<'a> {
    pub fn request<T>(&mut self, request: Request<T>) -> TestResponse<'_>
    where
        T: Into<Bytes>,
    {
        let respond = self.service.respond(request.map(RequestBody::new));
        let response = self
            .server
            .runtime
            .block_on(respond)
            .expect("AppService::call never fails");
        let (parts, body) = response.into_parts();
        TestResponse {
            response: Response::from_parts(parts, ()),
            body: Some(body),
            server: &mut *self.server,
        }
    }

    pub fn get(&mut self, uri: &str) -> TestResponse<'_> {
        self.request(Request::get(uri).body("").expect("valid request"))
    }
}

pub struct TestResponse<'a> {
    response: Response<()>,
    body: Option<ResponseBody>,
    server: &'a mut TestServer,
}

impl<'a> std::ops::Deref for TestResponse<'a> {
    type Target = Response<()>;

    fn deref(&self) -> &Self::Target {
        &self.response
    }
}

impl<'a> TestResponse<'a> {
    pub fn assert<A>(mut self, loc: Location, assertion: A) -> Result<Self>
    where
        A: Assertion,
    {
        match assertion.assert(&mut self) {
            Ok(()) => Ok(self),
            Err(kind) => Err(AssertionError { kind, loc }.into()),
        }
    }

    pub fn into_bytes(mut self) -> Result<Vec<u8>> {
        let body = self.body.take().expect("the response body has gone");
        self.server
            .runtime
            .block_on(body.into_buf_stream().collect::<Vec<u8>>())
            .map_err(|_| failure::format_err!("failed to collect body").into())
    }
}

#[derive(Debug)]
pub struct AssertionError {
    kind: AssertionErrorKind,
    loc: Location,
}

#[derive(Debug)]
pub enum AssertionErrorKind {
    Mismatched { expected: String, actual: String },
    MissingHeader { name: String },
    Msg(String),
}

impl fmt::Display for AssertionError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "\n[{}:{}] ", self.loc.file, self.loc.line)?;
        match self.kind {
            AssertionErrorKind::Mismatched {
                ref expected,
                ref actual,
            } => write!(f, "mismatched: expected={}, actual={}", expected, actual),
            AssertionErrorKind::MissingHeader { ref name } => write!(f, "missing header: {}", name),
            AssertionErrorKind::Msg(ref msg) => f.write_str(&**msg),
        }
    }
}

impl std::error::Error for AssertionError {}

pub trait Assertion {
    fn assert(self, res: &mut TestResponse<'_>) -> std::result::Result<(), AssertionErrorKind>;
}

pub mod status {
    use super::*;
    use http::StatusCode;

    impl Assertion for StatusCode {
        fn assert(
            self,
            response: &mut TestResponse<'_>,
        ) -> std::result::Result<(), AssertionErrorKind> {
            if response.response.status() == self {
                Ok(())
            } else {
                Err(AssertionErrorKind::Mismatched {
                    expected: format!("{:?}", self),
                    actual: format!("{:?}", response.response.status()),
                })
            }
        }
    }
}

pub mod header {
    use super::*;
    use http::header::{HeaderName, HeaderValue};

    pub fn eq<T>(name: HeaderName, value: T) -> EqHeader<T>
    where
        T: PartialEq<HeaderValue> + fmt::Debug,
    {
        EqHeader { name, value }
    }

    pub struct EqHeader<T> {
        name: HeaderName,
        value: T,
    }

    impl<T> Assertion for EqHeader<T>
    where
        T: PartialEq<HeaderValue> + fmt::Debug,
    {
        fn assert(
            self,
            response: &mut TestResponse<'_>,
        ) -> std::result::Result<(), AssertionErrorKind> {
            match response.response.headers().get(&self.name) {
                Some(ref h) if self.value == **h => Ok(()),
                Some(h) => Err(AssertionErrorKind::Mismatched {
                    expected: format!("{:?}", self.value),
                    actual: format!("{:?}", h),
                }),
                None => Err(AssertionErrorKind::MissingHeader {
                    name: self.name.to_string(),
                }),
            }
        }
    }

    pub fn not_exists(name: HeaderName) -> NotExists {
        NotExists(name)
    }

    pub struct NotExists(HeaderName);

    impl Assertion for NotExists {
        fn assert(
            self,
            response: &mut TestResponse<'_>,
        ) -> std::result::Result<(), AssertionErrorKind> {
            if !response.response.headers().contains_key(&self.0) {
                Ok(())
            } else {
                Err(AssertionErrorKind::Msg(format!(
                    "unexpected header field: `{}'",
                    self.0
                )))
            }
        }
    }
}

pub mod body {
    use super::*;
    use tokio_buf::BufStreamExt;

    pub fn eq<T>(expected: T) -> EqBody<T>
    where
        T: AsRef<[u8]> + fmt::Debug,
    {
        EqBody(expected)
    }

    pub struct EqBody<T>(T);

    impl<T> Assertion for EqBody<T>
    where
        T: AsRef<[u8]> + fmt::Debug,
    {
        fn assert(self, res: &mut TestResponse<'_>) -> std::result::Result<(), AssertionErrorKind> {
            let body = res.body.take().expect("the body has already been taken");
            let actual = res
                .server
                .runtime
                .block_on(body.collect::<Vec<u8>>())
                .expect("TODO: return error value");

            if self.0.as_ref() == &*actual {
                Ok(())
            } else {
                Err(AssertionErrorKind::Mismatched {
                    expected: format!("{:?}", self.0),
                    actual: format!(
                        "{:?}",
                        std::str::from_utf8(&actual).unwrap_or("<non-UTF-8 binary>")
                    ),
                })
            }
        }
    }
}
