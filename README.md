<img src="https://tsukuyomi-rs.github.io/images/tsukuyomi-header.png" alt="header" width="500" />

> Asynchronous Web framework for Rust.

---

[![Crates.io][crates-io-badge]][crates-io]
[![Minimal Rust Version: 1.31.0][rust-version-badge]][rust-version]
[![dependency status][deps-rs-badge]][deps-rs]
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

```toml
[dependencies]
tsukuyomi = "0.5.0-dev"
tsukuyomi-server = "0.2.0-dev"
```

```rust,no_run
use {
    std::net::SocketAddr,
    tsukuyomi::{
        App,
        config::prelude::*,
    },
    tsukuyomi_server::Server,
};

fn main() -> tsukuyomi_server::Result<()> {
    let app = App::create(
        path!("/")
            .to(endpoint::any()
                .reply("Hello, world.\n"))
    )?;

    let addr = SocketAddr::from(([127, 0, 0, 1], 4000));
    println!("Listening on http://{}", addr);

    Server::new(app).bind(addr).run()
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
[rust-version]: https://www.rust-lang.org
[master-doc]: https://tsukuyomi-rs.github.io/tsukuyomi
[gitter]: https://gitter.im/ubnt-intrepid/tsukuyomi
[examples]: https://github.com/tsukuyomi-rs/examples
[deps-rs]: https://deps.rs/crate/tsukuyomi/0.4.0
[azure-pipelines]: https://dev.azure.com/tsukuyomi-rs/tsukuyomi-rs/_build/latest?definitionId=1
[codecov]: https://codecov.io/gh/tsukuyomi-rs/tsukuyomi

[crates-io-badge]: https://img.shields.io/crates/v/tsukuyomi.svg
[rust-version-badge]: https://img.shields.io/badge/rustc-1.31.0+-yellow.svg
[gitter-badge]: https://badges.gitter.im/ubnt-intrepid/tsukuyomi.svg
[deps-rs-badge]: https://deps.rs/crate/tsukuyomi/0.4.0/status.svg
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
