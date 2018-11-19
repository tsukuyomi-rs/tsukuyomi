<img src="https://tsukuyomi-rs.github.io/images/tsukuyomi-header.png" alt="header" width="500" />

> Asynchronous Web framework for Rust.

---

[![Crates.io][crates-io-badge]][crates-io]
[![Minimal Rust Version: 1.30.0][rust-version-badge]][rust-version]
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
tsukuyomi = "0.4.0-dev"
```

```rust,no_run
extern crate tsukuyomi;

fn main() -> tsukuyomi::server::Result<()> {
    let server = tsukuyomi::app!()
        .route(
            tsukuyomi::app::route!()
                .reply(|| "Hello, world.\n")
        )
        .build_server()?;

    server
        .bind(std::net::SocketAddr::from(([127, 0, 0, 1], 4000)))
        .run_forever()
}
```

## Resources

* [Examples](./examples)
* [API documentation (released, 0.3)][docs-rs]
* [API documentation (master, 0.4)][master-doc]

## Extensions

- [`tsukuyomi-askama`] - template support using [`askama`]
- [`tsukuyomi-juniper`] - GraphQL integration using [`juniper`]
- [`tsukuyomi-fs`] - serving static files
- [`tsukuyomi-session`] - session management
- [`tsukuyomi-websocket`] - WebSocket support using [`tungstenite`]

## License
Tsukuyomi is licensed under either of [MIT license](LICENSE-MIT) or [Apache License, Version 2.0](LICENSE-APACHE) at your option.

<!-- links -->

[crates-io]: https://crates.io/crates/tsukuyomi
[docs-rs]: https://docs.rs/tsukuyomi
[rust-version]: https://www.rust-lang.org
[master-doc]: https://tsukuyomi-rs.github.io/tsukuyomi
[gitter]: https://gitter.im/ubnt-intrepid/tsukuyomi
[examples]: https://github.com/tsukuyomi-rs/examples
[deps-rs]: https://deps.rs/crate/tsukuyomi/0.4.0-dev
[azure-pipelines]: https://dev.azure.com/tsukuyomi-rs/tsukuyomi-rs/_build/latest?definitionId=1
[codecov]: https://codecov.io/gh/tsukuyomi-rs/tsukuyomi

[crates-io-badge]: https://img.shields.io/crates/v/tsukuyomi.svg
[rust-version-badge]: https://img.shields.io/badge/rustc-1.30.0+-lightgray.svg
[gitter-badge]: https://badges.gitter.im/ubnt-intrepid/tsukuyomi.svg
[deps-rs-badge]: https://deps.rs/crate/tsukuyomi/0.4.0-dev/status.svg
[azure-pipelines-badge]: https://dev.azure.com/tsukuyomi-rs/tsukuyomi-rs/_apis/build/status/tsukuyomi-rs.tsukuyomi
[codecov-badge]: https://codecov.io/gh/tsukuyomi-rs/tsukuyomi/branch/master/graph/badge.svg

[`askama`]: https://github.com/djc/askama
[`juniper`]: https://github.com/graphql-rust/juniper
[`tungstenite`]: https://github.com/snapview/tungstenite-rs

[`tsukuyomi-askama`]: ./tsukuyomi-askama
[`tsukuyomi-juniper`]: ./tsukuyomi-juniper
[`tsukuyomi-fs`]: ./tsukuyomi-fs
[`tsukuyomi-session`]: ./tsukuyomi-session
[`tsukuyomi-websocket`]: ./tsukuyomi-websocket
