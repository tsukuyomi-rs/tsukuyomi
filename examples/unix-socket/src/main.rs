extern crate tsukuyomi;

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

    tsukuyomi::app!()
        .route(
            tsukuyomi::app::route!("/") //
                .reply(|| "Hello, Tsukuyomi!\n"),
        ) //
        .build_server()?
        .bind(sock_path)
        .run()
}
