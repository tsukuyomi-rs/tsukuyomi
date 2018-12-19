use {
    tsukuyomi::{
        config::prelude::*, //
        App,
    },
    tsukuyomi_server::Server,
};

#[cfg(not(unix))]
fn main() {
    println!("This example works only on Unix platform.");
}

#[cfg(unix)]
fn main() -> tsukuyomi_server::Result<()> {
    let sock_path: std::path::PathBuf = std::env::args()
        .nth(1)
        .map(Into::into)
        .unwrap_or_else(|| "/tmp/tsukuyomi-uds.sock".into());

    App::create(
        path!("/") //
            .to(endpoint::any() //
                .reply("Hello, Tsukuyomi!\n")),
    )
    .map(App::into_service)
    .map(Server::new)?
    .bind(sock_path)
    .run()
}
