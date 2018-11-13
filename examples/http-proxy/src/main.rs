#![cfg_attr(feature = "cargo-clippy", allow(needless_pass_by_value))]

extern crate futures;
extern crate http;
extern crate reqwest;
extern crate tsukuyomi;

mod proxy;

use futures::prelude::*;
use tsukuyomi::app::route;

use crate::proxy::Client;

fn main() {
    let proxy_client =
        std::sync::Arc::new(crate::proxy::proxy_client(reqwest::async::Client::new()));

    let app = tsukuyomi::app()
        .route(
            route!("/")
                .with(proxy_client.clone())
                .handle(|client: Client| {
                    client
                        .send_forwarded_request("http://www.example.com")
                        .and_then(|resp| resp.receive_all())
                }),
        ) //
        .route(
            route!("/streaming")
                .with(proxy_client)
                .handle(|client: Client| {
                    client.send_forwarded_request("https://www.rust-lang.org/en-US/")
                }),
        ) //
        .finish()
        .unwrap();

    tsukuyomi::server(app).run_forever().unwrap();
}
