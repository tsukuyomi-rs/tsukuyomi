#![allow(clippy::needless_pass_by_value)]
#![recursion_limit = "128"]

mod proxy;

use {
    crate::proxy::Client, //
    futures::prelude::*,
    tsukuyomi::{
        config::prelude::*, //
        App,
        Server,
    },
};

fn main() -> tsukuyomi::server::Result<()> {
    let proxy_client =
        std::sync::Arc::new(crate::proxy::proxy_client(reqwest::r#async::Client::new()));

    App::create(chain![
        path!(/) //
            .to(endpoint::any()
                .extract(proxy_client.clone())
                .call_async(|client: Client| client
                    .send_forwarded_request("http://www.example.com")
                    .and_then(|resp| resp.receive_all()))),
        path!(/"streaming") //
            .to(endpoint::any()
                .extract(proxy_client)
                .call_async(|client: Client| client
                    .send_forwarded_request("https://www.rust-lang.org/en-US/"))),
    ]) //
    .map(Server::new)?
    .run()
}
