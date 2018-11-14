extern crate log;
extern crate pretty_env_logger;
extern crate tower_web;
extern crate tsukuyomi;

fn main() {
    let app = tsukuyomi::app!()
        .route(tsukuyomi::route!("/").reply(|| "Hello"))
        .finish()
        .unwrap();

    let addr: std::net::SocketAddr = "127.0.0.1:4000".parse().unwrap();

    std::env::set_var("RUST_LOG", "info");
    pretty_env_logger::init();
    log::info!("Listening on {}", addr);

    let log_middleware = tower_web::middleware::log::LogMiddleware::new(module_path!());

    tsukuyomi::server(app)
        .bind(addr)
        .with(log_middleware)
        .run_forever()
        .unwrap();
}
