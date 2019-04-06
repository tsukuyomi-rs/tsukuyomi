#![allow(clippy::needless_pass_by_value)]
#![recursion_limit = "128"]

mod proxy;

use {
    crate::proxy::Client, //
    futures::prelude::*,
    tsukuyomi::{endpoint, server::Server, App},
};

fn main() -> Result<(), exitfailure::ExitFailure> {
    let proxy_client =
        std::sync::Arc::new(crate::proxy::proxy_client(reqwest::r#async::Client::new()));

    let app = App::builder()
        .root(|mut scope| {
            scope
                .at("/")?
                .any()
                .extract(proxy_client.clone())
                .to(endpoint::call_async(|client: Client| {
                    client
                        .send_forwarded_request("http://www.example.com")
                        .and_then(|resp| resp.receive_all())
                }))?;

            scope
                .at("/streaming")?
                .any()
                .extract(proxy_client)
                .to(endpoint::call_async(|client: Client| {
                    client.send_forwarded_request("https://www.rust-lang.org/en-US/")
                }))
        })?
        .build()?;

    let mut server = Server::new(app)?;
    server.bind("127.0.0.1:4000")?;
    server.run_forever();

    Ok(())
}
