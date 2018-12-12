use tsukuyomi::{
    app::config::prelude::*, //
    server::Server,
    App,
};

fn main() -> tsukuyomi::server::Result<()> {
    let addr: std::net::SocketAddr = "127.0.0.1:4000".parse()?;

    let log_middleware = tower_web::middleware::log::LogMiddleware::new(module_path!());

    std::env::set_var("RUST_LOG", "info");
    pretty_env_logger::init();
    log::info!("Listening on {}", addr);

    App::configure(
        route() //
            .to(endpoint::any() //
                .say("Hello")),
    )
    .map(Server::new)?
    .bind(addr)
    .tower_middleware(log_middleware)
    .run()
}
