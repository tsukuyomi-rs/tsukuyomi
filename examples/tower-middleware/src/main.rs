extern crate log;
extern crate pretty_env_logger;
extern crate tower_web;
extern crate tsukuyomi;

fn main() -> tsukuyomi::server::Result<()> {
    let addr: std::net::SocketAddr = "127.0.0.1:4000".parse()?;

    let log_middleware = tower_web::middleware::log::LogMiddleware::new(module_path!());

    std::env::set_var("RUST_LOG", "info");
    pretty_env_logger::init();
    log::info!("Listening on {}", addr);

    tsukuyomi::app!()
        .route(tsukuyomi::route!("/").say("Hello"))
        .build_server()?
        .bind(addr)
        .tower_middleware(log_middleware)
        .run()
}
