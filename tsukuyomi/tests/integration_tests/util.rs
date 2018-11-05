use tsukuyomi::app::{App, Scope};

use tsukuyomi::local::{Data, Input as TestInput, LocalServer};
use tsukuyomi::server::CritError;
use tsukuyomi::service::http::{HttpRequest, HttpResponse};
use tsukuyomi::service::{NewService, Service};

use http::Response;

pub fn local_server<F>(f: F) -> LocalServer<App>
where
    F: FnOnce(&mut Scope<'_>),
{
    let app = App::build(f).expect("failed to construct App");
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
