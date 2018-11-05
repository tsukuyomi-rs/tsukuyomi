use tsukuyomi::app::builder::AppBuilder;
use tsukuyomi::app::App;
use tsukuyomi::handler::{Handle, Handler};
use tsukuyomi::input::Input;
use tsukuyomi::output::Responder;

use tsukuyomi::local::{Data, Input as TestInput, LocalServer};
use tsukuyomi::server::CritError;
use tsukuyomi::service::http::{HttpRequest, HttpResponse};
use tsukuyomi::service::{NewService, Service};

use http::Response;

pub fn local_server(app: AppBuilder) -> LocalServer<App> {
    let app = app.finish().expect("failed to construct App");
    LocalServer::new(app).expect("failed to initialize LocalServer")
}

pub trait LocalServerExt {
    fn perform(&mut self, input: impl TestInput) -> Result<Response<Data>, CritError>;
}

impl<S> LocalServerExt for LocalServer<S>
where
    S: NewService + Send + 'static,
    S::Request: HttpRequest,
    S::Response: HttpResponse,
    S::Error: Into<CritError>,
    S::Future: Send + 'static,
    S::Service: Send + 'static,
    <S::Service as Service>::Future: Send + 'static,
    S::InitError: Send + 'static + Into<CritError>,
{
    fn perform(&mut self, input: impl TestInput) -> Result<Response<Data>, CritError> {
        let mut client = self.client().map_err(Into::into)?;
        client.perform(input).map_err(Into::into)
    }
}

pub fn wrap_ready<R>(f: impl Fn(&mut Input<'_>) -> R) -> impl Handler
where
    R: Responder,
{
    #[allow(missing_debug_implementations)]
    struct ReadyHandler<T>(T);

    impl<T, R> Handler for ReadyHandler<T>
    where
        T: Fn(&mut Input<'_>) -> R,
        R: Responder,
    {
        fn handle(&self, input: &mut Input<'_>) -> Handle {
            Handle::ready(
                (self.0)(input)
                    .respond_to(input)
                    .map(|res| res.map(Into::into))
                    .map_err(Into::into),
            )
        }
    }

    ReadyHandler(f)
}
