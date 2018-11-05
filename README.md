<img src="https://raw.githubusercontent.com/tsukuyomi-rs/tsukuyomi/master/tsukuyomi-header.png" alt="header" width="500" />

> An asynchronous Web framework for Rust.

---

[![Crates.io][crates-io-badge]][crates-io]
[![Minimal Rust Version: 1.30.0][rust-version-badge]][rust-version]
[![dependency status][deps-rs-badge]][deps-rs]
[![Build Status][azure-pipelines-badge]][azure-pipelines]
[![Coverage Status][codecov-badge]][codecov]
[![Gitter][gitter-badge]][gitter]

## Documentation

* [Released][docs-rs]
* [Master][master-doc]

## Features

* Supports HTTP/1.x and HTTP/2.0 protocols, based on Hyper 0.12
* Basic support for HTTP/1.1 protocol upgrade
* TLS support by using `rustls`
* Support for both TCP and Unix domain socket
* Custom error handling
* Basic support for Cookie management
* Middlewares
* Embedded WebSocket handling

The following features does not currently implemented but will be supported in the future version:

* Custom session storage
* Authentication

## Usage

```toml
[dependencies]
tsukuyomi = "0.4.0-dev"
```

```rust,no_run
extern crate tsukuyomi;

fn main() {
    let app = tsukuyomi::app(|scope| {
        scope.route(
            tsukuyomi::route::index()
                .reply(|| "Hello, world.\n")
        );
    }).expect("failed to construct App");

    tsukuyomi::server(app)
        .bind("127.0.0.1:4000")
        .run_forever()
        .expect("failed to start the server");
}
```


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
