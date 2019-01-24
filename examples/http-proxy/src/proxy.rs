use {
    futures::prelude::*,
    http::header::{Entry, HeaderMap},
    izanami::RemoteAddr,
    reqwest::IntoUrl,
    std::mem,
    tsukuyomi::{
        chain,
        extractor::{self, ExtractorExt}, //
        future::TryFuture,
        output::IntoResponse,
        util::Never,
        Error,
        Extractor,
    },
};

#[derive(Debug)]
pub struct Client {
    client: reqwest::r#async::Client,
    headers: HeaderMap,
    remote_addr: RemoteAddr,
}

impl Client {
    pub fn send_forwarded_request(
        self,
        url: impl IntoUrl,
    ) -> impl Future<Error = Error, Item = ProxyResponse> {
        let Self {
            client,
            mut headers,
            remote_addr,
        } = self;

        headers.remove("host");

        match headers
            .entry("x-forwarded-for")
            .expect("should be a valid header name")
        {
            Entry::Occupied(mut entry) => {
                let addrs = match remote_addr.as_tcp() {
                    Some(remote_addr) => {
                        format!("{}, {}", entry.get().to_str().unwrap(), remote_addr)
                            .parse()
                            .unwrap()
                    }
                    None => entry.get().clone(),
                };
                entry.insert(addrs);
            }
            Entry::Vacant(entry) => {
                if let Some(remote_addr) = remote_addr.as_tcp() {
                    entry.insert(remote_addr.to_string().parse().unwrap());
                }
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
    resp: reqwest::r#async::Response,
}

impl ProxyResponse {
    pub fn receive_all(mut self) -> impl Future<Error = Error, Item = impl IntoResponse> {
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
            })
            .map(move |chunks| response.map(|_| chunks))
            .map_err(tsukuyomi::error::internal_server_error)
    }
}

impl IntoResponse for ProxyResponse {
    type Body = tsukuyomi::output::ResponseBody;
    type Error = Never;

    fn into_response(
        mut self,
        _: &http::Request<()>,
    ) -> Result<http::Response<Self::Body>, Self::Error> {
        let mut response = http::Response::new(());
        *response.status_mut() = self.resp.status();
        mem::swap(response.headers_mut(), self.resp.headers_mut());

        let body_stream = tsukuyomi::output::ResponseBody::wrap_stream(self.resp.into_body());

        Ok(response.map(|_| body_stream))
    }
}

pub fn proxy_client(
    client: reqwest::r#async::Client,
) -> impl Extractor<
    Output = (Client,), //
    Error = tsukuyomi::Error,
    Extract = impl TryFuture<Ok = (Client,), Error = tsukuyomi::Error> + Send + 'static,
> {
    chain![
        extractor::extension(),
        extractor::header::headers(),
        extractor::value(client),
    ]
    .map(|remote_addr, headers, client| Client {
        client,
        headers,
        remote_addr,
    })
}
