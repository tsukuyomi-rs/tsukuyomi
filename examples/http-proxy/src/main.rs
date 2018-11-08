extern crate bytes;
extern crate futures;
extern crate http;
extern crate reqwest;
extern crate tsukuyomi;

use bytes::{Buf, Bytes};
use futures::prelude::*;

use tsukuyomi::error::Error;
use tsukuyomi::output::Responder;

fn index() -> impl Future<Item = impl Responder, Error = Error> {
    reqwest::async::Client::new()
        .get("http://www.example.com")
        .send()
        .and_then(|mut resp| {
            let mut response = http::Response::new(());
            *response.status_mut() = resp.status();
            response
                .headers_mut()
                .extend(std::mem::replace(resp.headers_mut(), Default::default()));
            resp.into_body()
                .concat2()
                .map(move |chunks| response.map(|_| chunks.collect::<Bytes>()))
        }).map_err(tsukuyomi::error::internal_server_error)
}

fn streaming() -> impl Future<Item = impl Responder, Error = Error> {
    reqwest::async::Client::new()
        .get("https://www.rust-lang.org/en-US/")
        .send()
        .map(|resp| {
            let mut response = http::Response::new(());
            *response.status_mut() = resp.status();
            response.headers_mut().extend(resp.headers().clone());
            response.map(|_| tsukuyomi::output::ResponseBody::wrap_stream(resp.into_body()))
        }).map_err(tsukuyomi::error::internal_server_error)
}

fn main() {
    let app = tsukuyomi::app(|scope| {
        scope.route(tsukuyomi::route("/").handle(index));
        scope.route(tsukuyomi::route("/streaming").handle(streaming));
    }).unwrap();

    tsukuyomi::server(app)
        .bind("127.0.0.1:4000")
        .run_forever()
        .unwrap();
}
