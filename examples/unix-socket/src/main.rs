extern crate tsukuyomi;

#[cfg(not(unix))]
fn main() {
    println!("This example works only on Unix platform.");
}

#[cfg(unix)]
fn main() {
    let sock_path: std::path::PathBuf = std::env::args()
        .nth(1)
        .map(Into::into)
        .unwrap_or_else(|| "/tmp/tsukuyomi-uds.sock".into());

    let server = tsukuyomi::app()
        .route(
            tsukuyomi::app::route!("/") //
                .reply(|| "Hello, Tsukuyomi!\n"),
        ) //
        .build_server()
        .unwrap();

    println!("Serving on {}...", sock_path.display());
    println!();
    println!("The test command is as follows:");
    println!();
    println!(
        "  $ curl --unix-socket {} http://localhost/",
        sock_path.display()
    );
    println!();

    server.bind(sock_path).run_forever().unwrap();
}
