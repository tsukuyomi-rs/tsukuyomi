use {
    tsukuyomi::{
        config::prelude::*, //
        vendor::http::StatusCode,
        App,
    },
    tsukuyomi_server::Server,
};

fn main() -> tsukuyomi_server::Result<()> {
    std::env::set_var("RUST_LOG", "info");
    pretty_env_logger::try_init()?;

    let log = logging::log("request_logging");

    let app = App::create(
        chain![
            path!("/").to(endpoint::get().reply("Hello.")),
            path!("*").to(endpoint::reply(Err::<(), _>(StatusCode::NOT_FOUND)))
        ]
        .modify(log),
    )?;

    let addr: std::net::SocketAddr = "127.0.0.1:4000".parse()?;

    log::info!("Listening on http://{}", addr);
    Server::new(app).bind(addr).run()
}

mod logging {
    use {
        std::time::Instant,
        tsukuyomi::{
            future::{Async, Poll, TryFuture},
            handler::{AllowedMethods, Handler, ModifyHandler},
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

        fn allowed_methods(&self) -> Option<&AllowedMethods> {
            self.inner.allowed_methods()
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
        H::Ok: IntoResponse,
    {
        type Ok = Response<ResponseBody>;
        type Error = Never;

        fn poll_ready(&mut self, input: &mut Input<'_>) -> Poll<Self::Ok, Self::Error> {
            let result = match self.inner.poll_ready(input) {
                Ok(Async::NotReady) => return Ok(Async::NotReady),
                Ok(Async::Ready(output)) => output.into_response(input.request).map_err(Into::into),
                Err(err) => Err(err.into()),
            };

            //
            let response = result
                .map(|response| response.map(Into::into))
                .unwrap_or_else(|e| e.into_response(input.request).map(Into::into));

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
