extern crate tsukuyomi;

use tsukuyomi::future::ready;
use tsukuyomi::server::Server;
use tsukuyomi::App;

fn main() -> tsukuyomi::AppResult<()> {
    let sock_path: std::path::PathBuf = std::env::args()
        .nth(1)
        .map(Into::into)
        .unwrap_or_else(|| "/tmp/tsukuyomi-uds.sock".into());

    let app = App::builder()
        .mount("/", |r| {
            r.get("/", || ready("Hello"));
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
