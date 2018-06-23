extern crate pretty_env_logger;
extern crate tsukuyomi;

use tsukuyomi::{App, Handler};

fn main() -> tsukuyomi::AppResult<()> {
    pretty_env_logger::init();

    let app = App::builder()
        .mount("/", |m| {
            m.get("/").handle(Handler::new_ready(|_| "Hello, world\n"));

            m.mount("/api/v1/", |m| {
                m.mount("/posts", |m| {
                    m.get("/:id").handle(Handler::new_ready(|input| {
                        format!("get_post(id = {})", &input.params()[0])
                    }));

                    m.get("/").handle(Handler::new_ready(|_| "list_posts"));

                    m.post("/").handle(Handler::new_ready(|_| "add_post"));
                });

                m.mount("/user", |m| {
                    m.get("/auth").handle(Handler::new_ready(|_| "Authentication"));
                });
            });

            m.get("/static/*path")
                .handle(Handler::new_ready(|input| format!("path = {}\n", &input.params()[0])));
        })
        .finish()?;

    tsukuyomi::run(app)
}
