#![cfg_attr(feature = "cargo-clippy", allow(needless_pass_by_value))]

extern crate futures;
extern crate http;
extern crate reqwest;
extern crate tsukuyomi;

mod proxy;

use crate::proxy::Client;
use futures::prelude::*;
use tsukuyomi::app::Route;

fn main() {
    let proxy_client =
        std::sync::Arc::new(crate::proxy::proxy_client(reqwest::async::Client::new()));

    let app = tsukuyomi::app(|scope| {
        scope.route(
            Route::index()
                .with(proxy_client.clone())
                .handle(|client: Client| {
                    client
                        .send_forwarded_request("http://www.example.com")
                        .and_then(|resp| resp.receive_all())
                }),
        );

        scope.route(
            Route::get("/streaming")
                .with(proxy_client)
                .handle(|client: Client| {
                    client.send_forwarded_request("https://www.rust-lang.org/en-US/")
                }),
        );
    }).unwrap();

    tsukuyomi::server(app).run_forever().unwrap();
}
