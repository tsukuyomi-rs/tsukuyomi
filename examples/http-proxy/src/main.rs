#![allow(clippy::needless_pass_by_value)]
#![recursion_limit = "128"]

mod proxy;

use {
    crate::proxy::{Client, PeerAddr}, //
    futures::{prelude::*, Poll},
    http::Request,
    tsukuyomi::{
        config::prelude::*, //
        App,
    },
    tsukuyomi_server::Server,
    tsukuyomi_service::{modify_service_ref, Service},
};

fn main() -> tsukuyomi_server::Result<()> {
    let proxy_client =
        std::sync::Arc::new(crate::proxy::proxy_client(reqwest::r#async::Client::new()));

    let app = App::create(chain![
        path!("/") //
            .to(endpoint::any()
                .extract(proxy_client.clone())
                .call_async(|client: Client| client
                    .send_forwarded_request("http://www.example.com")
                    .and_then(|resp| resp.receive_all()))),
        path!("/streaming") //
            .to(endpoint::any()
                .extract(proxy_client)
                .call_async(|client: Client| client
                    .send_forwarded_request("https://www.rust-lang.org/en-US/"))),
    ])?;

    let service = app.into_service_with(modify_service_ref(
        |service, io: &tokio::net::TcpStream| -> std::io::Result<_> {
            #[allow(missing_debug_implementations)]
            struct WithPeerAddr<S> {
                service: S,
                peer_addr: PeerAddr,
            }

            impl<S, Bd> Service<Request<Bd>> for WithPeerAddr<S>
            where
                S: Service<Request<Bd>>,
            {
                type Response = S::Response;
                type Error = S::Error;
                type Future = S::Future;

                #[inline]
                fn poll_ready(&mut self) -> Poll<(), Self::Error> {
                    self.service.poll_ready()
                }

                #[inline]
                fn call(&mut self, mut request: Request<Bd>) -> Self::Future {
                    request.extensions_mut().insert(self.peer_addr.clone());
                    self.service.call(request)
                }
            }

            Ok(WithPeerAddr {
                service,
                peer_addr: io.peer_addr().map(PeerAddr)?,
            })
        },
    ));

    Server::new(service).run()
}
