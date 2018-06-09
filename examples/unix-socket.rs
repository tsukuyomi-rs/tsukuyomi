extern crate ganymede;
extern crate http;

#[cfg(unix)]
fn main() -> ganymede::AppResult<()> {
    use ganymede::server::Server;
    use ganymede::{App, Route};
    use http::Method;

    let sock_path: std::path::PathBuf = std::env::args()
        .nth(1)
        .map(Into::into)
        .unwrap_or_else(|| "/tmp/ganymede-uds.sock".into());

    let app = App::builder()
        .mount("/", vec![Route::new("/", Method::GET, |_: &_| Ok("Hello"))])
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
    println!("  $ curl --unix-socket /tmp/ganymede-uds.sock http://localhost/");
    println!();
    server.serve();

    Ok(())
}

#[cfg(not(unix))]
fn main() {
    println!("This example works only on Unix platform.");
}
