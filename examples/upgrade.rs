extern crate futures;
extern crate ganymede;
extern crate http;
extern crate pretty_env_logger;
extern crate tokio_io;

use ganymede::upgrade::{Upgrade, UpgradeContext};
use ganymede::{App, Context, Error, Route};

use futures::prelude::*;
use futures::stream;
use http::Method;
use tokio_io::codec::{Framed, FramedParts, LinesCodec};

fn handler(cx: UpgradeContext) -> impl Future<Item = (), Error = ()> + Send + 'static {
    let parts = FramedParts {
        inner: cx.io,
        readbuf: cx.read_buf.into(),
        writebuf: Default::default(),
    };
    let (sink, stream) = Framed::from_parts(parts, LinesCodec::new()).split();

    let lines = stream
        .take_while(|line| Ok(!line.is_empty()))
        .map_err(|_| ())
        .chain(stream::once(Ok("bye.".into())));

    sink.sink_map_err(|_| ()).send_all(lines).map(|_| ())
}

fn handshake(_cx: &Context) -> Result<Upgrade, Error> {
    // TODO: validate request
    Ok(Upgrade::builder("lines").finish(handler))
}

fn main() -> ganymede::app::Result<()> {
    pretty_env_logger::init();

    let app = App::builder()
        .mount("/", vec![Route::new("/", Method::GET, handshake)])
        .finish()?;
    ganymede::server::run(app)?;

    Ok(())
}
