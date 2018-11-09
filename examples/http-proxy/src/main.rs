#![cfg_attr(feature = "cargo-clippy", allow(needless_pass_by_value))]

extern crate bytes;
extern crate futures;
extern crate http;
extern crate reqwest;
extern crate tsukuyomi;

use std::mem;

use bytes::{Buf, Bytes};
use futures::prelude::*;
use reqwest::async::Client;

use tsukuyomi::error::Error;
use tsukuyomi::extractor;
use tsukuyomi::output::Responder;

fn index(client: Client) -> impl Future<Item = impl Responder, Error = Error> {
    client
        .get("http://www.example.com")
        .send()
        .and_then(|mut resp| {
            let mut response = http::Response::new(());
            *response.status_mut() = resp.status();
            mem::swap(response.headers_mut(), resp.headers_mut());
            resp.into_body()
                .concat2()
                .map(move |chunks| response.map(|_| chunks.collect::<Bytes>()))
        }).map_err(tsukuyomi::error::internal_server_error)
}

fn streaming(client: Client) -> impl Future<Item = impl Responder, Error = Error> {
    client
        .get("https://www.rust-lang.org/en-US/")
        .send()
        .map(|mut resp| {
            let mut response = http::Response::new(());
            *response.status_mut() = resp.status();
            mem::swap(response.headers_mut(), resp.headers_mut());
            response.map(|_| tsukuyomi::output::ResponseBody::wrap_stream(resp.into_body()))
        }).map_err(tsukuyomi::error::internal_server_error)
}

fn main() {
    let app = tsukuyomi::app(|scope| {
        scope.state(Client::new());

        scope.route(
            tsukuyomi::route("/")
                .with(extractor::state::cloned())
                .handle(index),
        );

        scope.route(
            tsukuyomi::route("/streaming")
                .with(extractor::state::cloned())
                .handle(streaming),
        );
    }).unwrap();

    tsukuyomi::server(app).run_forever().unwrap();
}
