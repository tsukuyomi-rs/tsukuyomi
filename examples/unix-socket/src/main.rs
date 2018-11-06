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

    let app = tsukuyomi::app(|scope| {
        scope.route(tsukuyomi::route::index().reply(|| "Hello, Tsukuyomi!\n"));
    }).unwrap();

    println!("Serving on {}...", sock_path.display());
    println!();
    println!("The test command is as follows:");
    println!();
    println!(
        "  $ curl --unix-socket {} http://localhost/",
        sock_path.display()
    );
    println!();
    tsukuyomi::server(app)
        .bind(sock_path)
        .run_forever()
        .unwrap();
}
