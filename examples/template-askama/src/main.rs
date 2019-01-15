use {
    askama::Template,
    izanami::Server,
    tsukuyomi::{
        config::prelude::*, //
        App,
        IntoResponse,
    },
};

#[derive(Template, IntoResponse)]
#[template(path = "index.html")]
#[response(preset = "tsukuyomi_askama::Askama")]
struct Index {
    name: String,
}

fn main() -> izanami::Result<()> {
    let app = App::create(
        path!("/:name") //
            .to(endpoint::call(|name| Index { name })),
    )?;

    Server::bind_tcp(&"127.0.0.1:4000".parse()?)? //
        .start(app)
}
