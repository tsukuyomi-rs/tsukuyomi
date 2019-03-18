#![allow(clippy::needless_pass_by_value)]
#![recursion_limit = "128"]

mod proxy;

use {
    crate::proxy::Client, //
    futures::prelude::*,
    tsukuyomi::{
        config::prelude::*, //
        server::Server,
        App,
    },
};

fn main() -> Result<(), exitfailure::ExitFailure> {
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

    let mut server = Server::new(app)?;
    server.bind("127.0.0.1:4000")?;
    server.run_forever();

    Ok(())
}
