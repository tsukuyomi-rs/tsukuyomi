use futures::{future, Future, Sink, Stream};
use http::{header, StatusCode};
use std::mem;
use tokio_codec::{Framed, FramedParts, LinesCodec};

use tsukuyomi::upgrade::{Upgrade, UpgradeContext};
use tsukuyomi::{Error, Input};

fn missing_header(name: &str) -> Error {
    Error::from_failure(
        format_err!("The header `{}' is missing", name),
        StatusCode::UPGRADE_REQUIRED,
    )
}

pub fn start<F>(cx: &Input, handler: F) -> Result<Upgrade, Error>
where
    F: Fn(String) -> Option<String> + Send + Sync + 'static,
{
    // validate the header Connection
    let h = cx.headers()
        .get(header::CONNECTION)
        .ok_or_else(|| missing_header("Connection"))?;
    if h.as_bytes() != b"Upgrade" {
        return Err(Error::bad_request(format_err!(
            "The value of header `Connection' must be 'Upgrade'"
        )));
    }

    // validate the header Upgrade
    let h = cx.headers()
        .get(header::UPGRADE)
        .ok_or_else(|| missing_header("Upgrade"))?;
    if h.as_bytes() != b"lines" {
        return Err(Error::bad_request(format_err!(
            "The value of header `Upgrade' must be 'lines'"
        )));
    }

    let upgrade = Upgrade::builder("lines").finish(|cx| build_upgrade_handler(cx, handler));
    Ok(upgrade)
}

fn build_upgrade_handler<F>(cx: UpgradeContext, handler: F) -> impl Future<Item = (), Error = ()> + Send + 'static
where
    F: Fn(String) -> Option<String> + Send + Sync + 'static,
{
    let mut parts = FramedParts::new(cx.io, LinesCodec::new());
    parts.read_buf = cx.read_buf.into();

    let (sink, stream) = Framed::from_parts(parts).split();

    future::lazy(|| Ok(()))
        .inspect(|_| info!("Start the connection"))
        .and_then(|_| {
            let input = stream
                .map_err(|_| ())
                .take_while(|line| Ok(!line.is_empty() && line != "bye."))
                .inspect(|line| info!("Received: {}", line))
                .map(move |line| handler(line))
                .inspect(|res| info!("Response: {:?}", res))
                .take_while(|s| Ok(s.is_some()))
                .map(Option::unwrap);

            sink.sink_map_err(|_| ())
                .send_all(input)
                .and_then(|(sink, _)| sink.send("bye".into()))
                .map(mem::drop)
        })
        .inspect(|_| info!("Shutdown the connection"))
}
