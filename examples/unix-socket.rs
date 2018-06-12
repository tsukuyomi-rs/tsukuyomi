extern crate tsukuyomi;

#[cfg(unix)]
fn main() -> tsukuyomi::AppResult<()> {
    use tsukuyomi::server::Server;
    use tsukuyomi::App;

    let sock_path: std::path::PathBuf = std::env::args()
        .nth(1)
        .map(Into::into)
        .unwrap_or_else(|| "/tmp/tsukuyomi-uds.sock".into());

    let app = App::builder()
        .mount("/", |r| {
            r.get("/", |_: &_| Ok("Hello"));
        })
        .finish()?;

    let server = Server::builder()
        .transport(|t| {
            t.bind_uds(&sock_path);
        })
        .finish(app)?;

    println!("Serving on {}...", sock_path.display());
    println!();
    println!("The test command is as follows:");
    println!();
    println!("  $ curl --unix-socket {} http://localhost/", sock_path.display());
    println!();
    server.serve();

    Ok(())
}

#[cfg(not(unix))]
fn main() {
    println!("This example works only on Unix platform.");
}
