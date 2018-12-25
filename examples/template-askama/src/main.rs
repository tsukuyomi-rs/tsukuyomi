use {
    askama::Template,
    tsukuyomi::{
        config::prelude::*, //
        App,
        IntoResponse,
    },
    tsukuyomi_server::Server,
};

#[derive(Template, IntoResponse)]
#[template(path = "index.html")]
#[response(preset = "tsukuyomi_askama::Askama")]
struct Index {
    name: String,
}

fn main() -> tsukuyomi_server::Result<()> {
    App::create(
        path!("/:name") //
            .to(endpoint::call(|name| Index { name })),
    )
    .map(Server::new)?
    .run()
}
