#![cfg_attr(feature = "cargo-clippy", allow(needless_pass_by_value))]

extern crate futures;
extern crate http;
extern crate reqwest;
extern crate tsukuyomi;

mod proxy;

use {crate::proxy::Client, futures::prelude::*, tsukuyomi::app::scope::route};

fn main() -> tsukuyomi::server::Result<()> {
    let proxy_client =
        std::sync::Arc::new(crate::proxy::proxy_client(reqwest::async::Client::new()));

    tsukuyomi::app!()
        .with(
            route!("/")
                .extract(proxy_client.clone())
                .call(|client: Client| {
                    client
                        .send_forwarded_request("http://www.example.com")
                        .and_then(|resp| resp.receive_all())
                }),
        ) //
        .with(
            route!("/streaming")
                .extract(proxy_client)
                .call(|client: Client| {
                    client.send_forwarded_request("https://www.rust-lang.org/en-US/")
                }),
        ) //
        .build_server()?
        .run()
}
