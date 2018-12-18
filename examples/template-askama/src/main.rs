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
#[response(with = "tsukuyomi_askama::into_response")]
struct Index {
    name: String,
}

fn main() -> tsukuyomi_server::Result<()> {
    App::create({
        path!("/:name") //
            .to({
                endpoint::get() //
                    .call(|name| Index { name })
            })
    })
    .map(Server::new)?
    .run()
}
