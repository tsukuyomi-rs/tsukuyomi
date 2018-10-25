use tsukuyomi::app::builder::AppBuilder;
use tsukuyomi::app::App;

use tsukuyomi::server::local::{Data, Input, LocalServer};
use tsukuyomi::server::service::http::{HttpRequest, HttpResponse};
use tsukuyomi::server::service::{NewService, Service};
use tsukuyomi::server::CritError;

use http::Response;

pub fn local_server(app: AppBuilder) -> LocalServer<App> {
    let app = app.finish().expect("failed to construct App");
    LocalServer::new(app).expect("failed to initialize LocalServer")
}

pub trait LocalServerExt {
    fn perform(&mut self, input: impl Input) -> Result<Response<Data>, CritError>;
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
    fn perform(&mut self, input: impl Input) -> Result<Response<Data>, CritError> {
        let mut client = self.client().map_err(Into::into)?;
        client.perform(input).map_err(Into::into)
    }
}
