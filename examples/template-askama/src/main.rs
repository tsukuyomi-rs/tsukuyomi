use {
    askama::Template,
    tsukuyomi::{
        app::config::prelude::*, //
        output::IntoResponse,
        App,
    },
};

#[derive(Template, IntoResponse)]
#[template(path = "index.html")]
#[response(with = "tsukuyomi_askama::into_response")]
struct Index {
    name: String,
}

fn main() -> tsukuyomi::server::Result<()> {
    App::create({
        path!(/{path::param("name")}) //
            .to({
                endpoint::get() //
                    .call(|name| Index { name })
            })
    })
    .map(tsukuyomi::server::Server::new)?
    .run()
}
