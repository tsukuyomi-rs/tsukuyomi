#![allow(clippy::needless_pass_by_value)]
#![recursion_limit = "128"]

mod peer;
mod proxy;

use {
    crate::proxy::Client, //
    futures::prelude::*,
    tsukuyomi::{
        config::prelude::*, //
        App,
    },
    tsukuyomi_server::Server,
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

    let app = app.with_modify_service(crate::peer::with_peer_addr());

    Server::new(app).run()
}
