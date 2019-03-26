use {
    askama::Template,
    exitfailure::ExitFailure,
    tsukuyomi::{
        config::prelude::*, //
        server::Server,
        App,
        Responder,
    },
};

#[derive(Template, Responder)]
#[template(path = "index.html")]
#[response(preset = "tsukuyomi_askama::Askama")]
struct Index {
    name: String,
}

fn main() -> Result<(), ExitFailure> {
    let app = App::create(
        path!("/:name") //
            .to(endpoint::call(|name| Index { name })),
    )?;

    let mut server = Server::new(app)?;
    server.bind("127.0.0.1:4000")?;
    server.run_forever();

    Ok(())
}
