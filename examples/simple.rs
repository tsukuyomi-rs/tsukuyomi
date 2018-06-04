extern crate ganymede;
extern crate http;
extern crate pretty_env_logger;

use ganymede::handler::handler_fn;
use http::Response;

fn main() -> ganymede::Result<()> {
    pretty_env_logger::init();

    let addr = ([127, 0, 0, 1], 4000).into();

    let handler = handler_fn(|_| {
        Response::builder()
            .body(Default::default())
            .map_err(Into::into)
    });

    ganymede::launch(handler, &addr)
}
