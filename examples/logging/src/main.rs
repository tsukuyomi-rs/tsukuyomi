use tsukuyomi::{endpoint, server::Server, vendor::http::StatusCode, App};

fn main() -> Result<(), exitfailure::ExitFailure> {
    std::env::set_var("RUST_LOG", "info");
    pretty_env_logger::try_init()?;

    let log = logging::log("request_logging");

    let app = App::builder()
        .root(|mut scope| {
            scope.with(&log).done(|mut scope| {
                scope.at("/")?.get().to(endpoint::call(|| "Hello."))?;

                scope.fallback(endpoint::call(|| StatusCode::NOT_FOUND))
            })
        })?
        .build()?;

    let mut server = Server::new(app)?;
    log::info!("Listening on http://127.0.0.1:4000");
    server.bind("127.0.0.1:4000")?;
    server.run_forever();

    Ok(())
}

mod logging {
    use {
        std::time::Instant,
        tsukuyomi::{
            future::{Async, Poll, TryFuture},
            handler::{Handler, ModifyHandler},
            input::Input,
            output::{IntoResponse, ResponseBody},
            util::Never,
            vendor::http::Response,
        },
    };

    pub fn log(target: &'static str) -> Logging {
        Logging { target }
    }

    pub struct Logging {
        target: &'static str,
    }

    impl<H> ModifyHandler<H> for Logging
    where
        H: Handler,
        H::Output: IntoResponse,
    {
        type Output = Response<ResponseBody>;
        type Error = Never;
        type Handler = WithLogging<H>;

        fn modify(&self, inner: H) -> Self::Handler {
            WithLogging {
                inner,
                target: self.target,
            }
        }
    }

    pub struct WithLogging<H> {
        inner: H,
        target: &'static str,
    }

    impl<H> Handler for WithLogging<H>
    where
        H: Handler,
        H::Output: IntoResponse,
    {
        type Output = Response<ResponseBody>;
        type Error = Never;
        type Handle = HandleWithLogging<H::Handle>;

        fn handle(&self) -> Self::Handle {
            HandleWithLogging {
                inner: self.inner.handle(),
                target: self.target,
                start: Instant::now(),
            }
        }
    }

    pub struct HandleWithLogging<H> {
        inner: H,
        target: &'static str,
        start: Instant,
    }

    impl<H> TryFuture for HandleWithLogging<H>
    where
        H: TryFuture,
        H::Ok: IntoResponse, // FIXME: switch to Responder
    {
        type Ok = Response<ResponseBody>;
        type Error = Never;

        fn poll_ready(&mut self, input: &mut Input<'_>) -> Poll<Self::Ok, Self::Error> {
            let result: tsukuyomi::Result<_> = match self.inner.poll_ready(input) {
                Ok(Async::NotReady) => return Ok(Async::NotReady),
                Ok(Async::Ready(output)) => Ok(output.into_response()),
                Err(err) => Err(err.into()),
            };

            //
            let response = result
                .map(|response| response.map(Into::into))
                .unwrap_or_else(|e| e.into_response());

            let log_level = match response.status().as_u16() {
                400...599 => log::Level::Error,
                _ => log::Level::Info,
            };
            log::log!(
                target: self.target,
                log_level,
                "\"{} {} {:?}\" -> \"{}\" ({:?})",
                input.request.method(),
                input.request.uri().path(),
                input.request.version(),
                response.status(),
                self.start.elapsed()
            );

            Ok(Async::Ready(response))
        }
    }
}
