use tsukuyomi::{
    app::config::prelude::*, //
    server::Server,
    App,
};

#[cfg(not(unix))]
fn main() {
    println!("This example works only on Unix platform.");
}

#[cfg(unix)]
fn main() -> tsukuyomi::server::Result<()> {
    let sock_path: std::path::PathBuf = std::env::args()
        .nth(1)
        .map(Into::into)
        .unwrap_or_else(|| "/tmp/tsukuyomi-uds.sock".into());

    App::create(
        path!(/) //
            .to(endpoint::any() //
                .reply("Hello, Tsukuyomi!\n")),
    )
    .map(Server::new)?
    .bind(sock_path)
    .run()
}
