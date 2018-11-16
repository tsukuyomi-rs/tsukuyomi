use std::mem;
use std::net::SocketAddr;

use futures::prelude::*;
use http::header::{Entry, HeaderMap};
use reqwest::IntoUrl;

use tsukuyomi::error::{Error, Never};
use tsukuyomi::extractor;
use tsukuyomi::extractor::Extractor;
use tsukuyomi::input::Input;
use tsukuyomi::output::Responder;

#[derive(Debug)]
pub struct Client {
    client: reqwest::async::Client,
    headers: HeaderMap,
    peer_addr: SocketAddr,
}

impl Client {
    pub fn send_forwarded_request(
        self,
        url: impl IntoUrl,
    ) -> impl Future<Error = Error, Item = ProxyResponse> {
        let Self {
            client,
            mut headers,
            peer_addr,
        } = self;

        headers.remove("host");

        match headers
            .entry("x-forwarded-for")
            .expect("should be a valid header name")
        {
            Entry::Occupied(mut entry) => {
                let addrs = format!("{}, {}", entry.get().to_str().unwrap(), peer_addr);
                entry.insert(addrs.parse().unwrap());
            }
            Entry::Vacant(entry) => {
                entry.insert(peer_addr.to_string().parse().unwrap());
            }
        }

        client
            .get(url)
            .headers(headers)
            .send()
            .map(|resp| ProxyResponse { resp })
            .map_err(tsukuyomi::error::internal_server_error)
    }
}

pub struct ProxyResponse {
    resp: reqwest::async::Response,
}

impl ProxyResponse {
    pub fn receive_all(mut self) -> impl Future<Error = Error, Item = impl Responder> {
        let mut response = http::Response::new(());
        *response.status_mut() = self.resp.status();
        mem::swap(response.headers_mut(), self.resp.headers_mut());

        let content_length = response
            .headers()
            .get("content-length")
            .and_then(|h| h.to_str().ok())
            .and_then(|s| s.parse::<usize>().ok())
            .unwrap_or(0);

        self.resp
            .into_body()
            .fold(Vec::with_capacity(content_length), |mut acc, chunk| {
                acc.extend_from_slice(&*chunk);
                Ok::<_, reqwest::Error>(acc)
            }).map(move |chunks| response.map(|_| chunks))
            .map_err(tsukuyomi::error::internal_server_error)
    }
}

impl Responder for ProxyResponse {
    type Body = tsukuyomi::output::ResponseBody;
    type Error = Never;

    fn respond_to(mut self, _: &mut Input<'_>) -> Result<http::Response<Self::Body>, Self::Error> {
        let mut response = http::Response::new(());
        *response.status_mut() = self.resp.status();
        mem::swap(response.headers_mut(), self.resp.headers_mut());

        let body_stream = tsukuyomi::output::ResponseBody::wrap_stream(self.resp.into_body());

        Ok(response.map(|_| body_stream))
    }
}

pub fn proxy_client(
    client: reqwest::async::Client,
) -> impl Extractor<Output = (Client,), Error = Error> {
    extractor::extension::clone()
        .into_builder() // <-- start building
        .and(extractor::header::clone_headers())
        .and(extractor::value(client))
        .map(|peer_addr, headers, client| Client {
            client,
            headers,
            peer_addr,
        })
}
