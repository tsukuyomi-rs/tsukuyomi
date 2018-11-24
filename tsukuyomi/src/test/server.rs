#![allow(unknown_lints)] // explicit_outlives_requirements

use {
    super::{
        input::Input,
        output::{Output, Receive},
    },
    cookie::Cookie,
    crate::server::{
        service::{HttpService, Identity, MakeHttpService, ModifyHttpService},
        CritError,
    },
    futures::{Future, Poll},
    http::{
        header::{COOKIE, SET_COOKIE},
        Request, Response,
    },
    hyper::body::Payload,
    std::{collections::HashMap, mem},
};

/// A test server which emulates an HTTP service without using the low-level I/O.
#[derive(Debug)]
pub struct Server<S, M = Identity, Rt = tokio::runtime::Runtime> {
    new_service: S,
    modify_service: M,
    runtime: Rt,
}

impl<S, M, Rt> Server<S, M, Rt>
where
    S: MakeHttpService,
    M: ModifyHttpService<S::Service>,
{
    /// Creates an instance of `TestServer` from the specified components.
    pub fn new(new_service: S, modify_service: M, runtime: Rt) -> Self {
        Self {
            new_service,
            modify_service,
            runtime,
        }
    }
}

/// A type which manages a series of requests.
#[derive(Debug)]
#[allow(explicit_outlives_requirements)]
pub struct Session<'a, S, Rt: 'a> {
    service: S,
    cookies: Option<HashMap<String, String>>,
    runtime: &'a mut Rt,
}

impl<'a, S, Rt> Session<'a, S, Rt>
where
    S: HttpService,
{
    fn new(service: S, runtime: &'a mut Rt) -> Self {
        Session {
            service,
            runtime,
            cookies: None,
        }
    }

    /// Sets whether to save the Cookie entries or not.
    ///
    /// The default value is `false`.
    pub fn save_cookies(mut self, enabled: bool) -> Self {
        if enabled {
            self.cookies.get_or_insert_with(Default::default);
        } else {
            self.cookies.take();
        }
        self
    }

    /// Returns the reference to the underlying Tokio runtime.
    pub fn runtime(&mut self) -> &mut Rt {
        &mut *self.runtime
    }

    fn build_request<T>(&self, input: T) -> super::Result<Request<S::RequestBody>>
    where
        T: Input,
    {
        let mut request = input.build_request()?.map(S::RequestBody::from);
        if let Some(cookies) = &self.cookies {
            for (k, v) in cookies {
                request.headers_mut().append(
                    COOKIE,
                    Cookie::new(k.to_owned(), v.to_owned())
                        .to_string()
                        .parse()?,
                );
            }
        }
        Ok(request)
    }

    fn handle_set_cookies(&mut self, response: &Response<Output>) -> super::Result<()> {
        if let Some(ref mut cookies) = &mut self.cookies {
            for set_cookie in response.headers().get_all(SET_COOKIE) {
                let cookie = Cookie::parse_encoded(set_cookie.to_str()?)?;
                if cookie.value().is_empty() {
                    cookies.remove(cookie.name());
                } else {
                    cookies.insert(cookie.name().to_owned(), cookie.value().to_owned());
                }
            }
        }
        Ok(())
    }
}

mod threadpool {
    use {
        super::*,
        std::panic::{resume_unwind, AssertUnwindSafe},
        tokio::runtime::Runtime,
    };

    fn block_on<F>(runtime: &mut Runtime, future: F) -> Result<F::Item, F::Error>
    where
        F: Future + Send + 'static,
        F::Item: Send + 'static,
        F::Error: Send + 'static,
    {
        match runtime.block_on(AssertUnwindSafe(future).catch_unwind()) {
            Ok(result) => result,
            Err(err) => resume_unwind(Box::new(err)),
        }
    }

    impl<S, M> Server<S, M, Runtime>
    where
        S: MakeHttpService,
        S::Future: Send + 'static,
        S::InitError: Send + 'static,
        S::Service: Send + 'static,
        M: ModifyHttpService<S::Service>,
    {
        /// Create a `Session` associated with this server.
        pub fn new_session(&mut self) -> super::super::Result<Session<'_, M::Service, Runtime>> {
            let service = block_on(
                &mut self.runtime,
                self.new_service.make_http_service().map_err(Into::into),
            ).map_err(failure::Error::from_boxed_compat)?;

            Ok(Session::new(
                self.modify_service.modify_http_service(service),
                &mut self.runtime,
            ))
        }

        pub fn perform<T>(&mut self, input: T) -> super::super::Result<Response<Output>>
        where
            T: Input,
            <M::Service as HttpService>::Future: Send + 'static,
        {
            let mut session = self.new_session()?;
            session.perform(input)
        }
    }

    impl<'a, S> Session<'a, S, Runtime>
    where
        S: HttpService,
        S::Future: Send + 'static,
    {
        /// Applies an HTTP request to this client and await its response.
        pub fn perform<T>(&mut self, input: T) -> super::super::Result<Response<Output>>
        where
            T: Input,
        {
            let request = self.build_request(input)?;

            let future = TestResponseFuture::Initial(self.service.call_http(request));
            let response =
                block_on(&mut self.runtime, future).map_err(failure::Error::from_boxed_compat)?;
            self.handle_set_cookies(&response)?;

            Ok(response)
        }
    }
}

mod current_thread {
    use {super::*, tokio::runtime::current_thread::Runtime};

    impl<S, M> Server<S, M, Runtime>
    where
        S: MakeHttpService,
        M: ModifyHttpService<S::Service>,
    {
        /// Create a `Session` associated with this server.
        pub fn new_session(&mut self) -> super::super::Result<Session<'_, M::Service, Runtime>> {
            let service = self
                .runtime
                .block_on(self.new_service.make_http_service())
                .map_err(|err| failure::Error::from_boxed_compat(err.into()))?;
            let service = self.modify_service.modify_http_service(service);
            Ok(Session::new(service, &mut self.runtime))
        }

        pub fn perform<T>(&mut self, input: T) -> super::super::Result<Response<Output>>
        where
            T: Input,
        {
            let mut session = self.new_session()?;
            session.perform(input)
        }
    }

    impl<'a, S> Session<'a, S, Runtime>
    where
        S: HttpService,
    {
        /// Applies an HTTP request to this client and await its response.
        pub fn perform<T>(&mut self, input: T) -> super::super::Result<Response<Output>>
        where
            T: Input,
        {
            let request = self.build_request(input)?;

            let future = TestResponseFuture::Initial(self.service.call_http(request));
            let response = self
                .runtime
                .block_on(future)
                .map_err(failure::Error::from_boxed_compat)?;
            self.handle_set_cookies(&response)?;

            Ok(response)
        }
    }
}

#[cfg_attr(feature = "cargo-clippy", allow(large_enum_variant))]
#[derive(Debug)]
enum TestResponseFuture<F, Bd: Payload> {
    Initial(F),
    Receive(http::response::Parts, Receive<Bd>),
    Done,
}

impl<F, Bd> Future for TestResponseFuture<F, Bd>
where
    F: Future<Item = Response<Bd>>,
    F::Error: Into<CritError>,
    Bd: Payload,
{
    type Item = Response<Output>;
    type Error = CritError;

    fn poll(&mut self) -> Poll<Self::Item, Self::Error> {
        use self::TestResponseFuture::*;
        loop {
            let response = match *self {
                Initial(ref mut f) => {
                    let response = futures::try_ready!(f.poll().map_err(Into::into));
                    Some(response)
                }
                Receive(_, ref mut receive) => {
                    futures::try_ready!(receive.poll_ready().map_err(Into::into));
                    None
                }
                _ => unreachable!("unexpected state"),
            };

            match mem::replace(self, TestResponseFuture::Done) {
                TestResponseFuture::Initial(..) => {
                    let response = response.expect("unexpected condition");
                    let (parts, body) = response.into_parts();
                    let receive = self::Receive::new(body);
                    *self = TestResponseFuture::Receive(parts, receive);
                }
                TestResponseFuture::Receive(parts, receive) => {
                    let data = receive.into_data().expect("unexpected condition");
                    let response = Response::from_parts(parts, data);
                    return Ok(response.into());
                }
                _ => unreachable!("unexpected state"),
            }
        }
    }
}
