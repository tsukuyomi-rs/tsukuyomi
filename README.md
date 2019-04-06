<img src="https://tsukuyomi-rs.github.io/images/tsukuyomi-header.png" alt="header" width="500" />

> Asynchronous Web framework for Rust.

---

[![Crates.io][crates-io-badge]][crates-io]
[![Minimal Rust Version: 1.31.0][rust-version-badge]][rust-version]
[![Build Status][azure-pipelines-badge]][azure-pipelines]
[![Coverage Status][codecov-badge]][codecov]
[![Gitter][gitter-badge]][gitter]

## Features

* Type-safe and composable handlers based on `Extractor` system
* Scoped routing and middlewares
* Asynchronous HTTP server based on `tokio`, `hyper` and `tower-service`
  - HTTP/1.1 protocol upgrade
  - Both of TCP and [Unix domain socket](./examples/unix-socket) support
  - TLS support (with [`native-tls`](./examples/native-tls), [`rustls`](./examples/rustls) or [`openssl`](./examples/openssl))

## Usage

```rust,no_run
use tsukuyomi::{App, server::Server};
use tsukuyomi::endpoint;

fn main() -> Result<(), Box<dyn std::error::Error + Send + Sync>> {
    let app = App::builder()
        .root(|mut scope| {
            scope.at("/")?
                .to(endpoint::call(|| "Hello, world.\n"))
        })?
        .build()?;

    let mut server = Server::new(app)?;

    println!("Listening on http://localhost:4000/");
    server.bind("127.0.0.1:4000")?;

    server.run_forever();
    Ok(())
}
```

## Resources

* [Examples](./examples)
* [API documentation (released)][docs-rs]
* [API documentation (master)][master-doc]

## Extensions

- [`tsukuyomi-askama`] - template support using [`askama`]
- [`tsukuyomi-cors`] - CORS support
- [`tsukuyomi-juniper`] - GraphQL integration using [`juniper`]
- [`tsukuyomi-session`] - session management
- [`tsukuyomi-tungstenite`] - WebSocket support using [`tungstenite`]

## License
Tsukuyomi is licensed under either of [MIT license](LICENSE-MIT) or [Apache License, Version 2.0](LICENSE-APACHE) at your option.

<!-- links -->

[crates-io]: https://crates.io/crates/tsukuyomi
[docs-rs]: https://docs.rs/tsukuyomi
[rust-version]: https://blog.rust-lang.org/2018/12/06/Rust-1.31-and-rust-2018.html
[master-doc]: https://tsukuyomi-rs.github.io/tsukuyomi
[gitter]: https://gitter.im/ubnt-intrepid/tsukuyomi
[examples]: https://github.com/tsukuyomi-rs/examples
[azure-pipelines]: https://dev.azure.com/tsukuyomi-rs/tsukuyomi-rs/_build/latest?definitionId=1
[codecov]: https://codecov.io/gh/tsukuyomi-rs/tsukuyomi

[crates-io-badge]: https://img.shields.io/crates/v/tsukuyomi.svg
[rust-version-badge]: https://img.shields.io/badge/rustc-1.31.0+-yellow.svg
[gitter-badge]: https://badges.gitter.im/ubnt-intrepid/tsukuyomi.svg
[azure-pipelines-badge]: https://dev.azure.com/tsukuyomi-rs/tsukuyomi-rs/_apis/build/status/tsukuyomi-rs.tsukuyomi
[codecov-badge]: https://codecov.io/gh/tsukuyomi-rs/tsukuyomi/branch/master/graph/badge.svg

[`askama`]: https://github.com/djc/askama
[`juniper`]: https://github.com/graphql-rust/juniper
[`tungstenite`]: https://github.com/snapview/tungstenite-rs

[`tsukuyomi-askama`]: ./tsukuyomi-askama
[`tsukuyomi-cors`]: ./tsukuyomi-cors
[`tsukuyomi-juniper`]: ./tsukuyomi-juniper
[`tsukuyomi-session`]: ./tsukuyomi-session
[`tsukuyomi-tungstenite`]: ./tsukuyomi-tungstenite
