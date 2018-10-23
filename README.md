<img src="https://raw.githubusercontent.com/tsukuyomi-rs/tsukuyomi/master/tsukuyomi-header.png" alt="header" width="500" />

> An Asynchronous Web framework for Rust.

---

[![Crates.io][crates-io-badge]][crates-io]
[![Crates.io (Downloads)][downloads-badge]][crates-io]
[![Docs.rs][docs-rs-badge]][docs-rs]
[![Master Doc][master-doc-badge]][master-doc]
[![Minimal Rust Version: 1.30.0][rust-version-badge]][rust-version]
[![dependency status][deps-rs-badge]][deps-rs]
[![Gitter][gitter-badge]][gitter]

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

## Documentation

* [Examples][examples]
* [API documentation (released)][docs-rs]
* [API documentation (master)][master-doc]

## Build Status

| Travis CI | Azure Pipelines | CircleCI | Codecov |
|:---------:|:---------------:|:--------:|:-------:|
| [![Build Status][travis-badge]][travis] | [![Build Status][azure-pipelines-badge]][azure-pipelines] | [![CircleCI][circleci-badge]][circleci] | [![Coverage Status][codecov-badge]][codecov] |

## License
Tsukuyomi is licensed under either of [MIT license](LICENSE-MIT) or [Apache License, Version 2.0](LICENSE-APACHE) at your option.

<!-- links -->

[crates-io]: https://crates.io/crates/tsukuyomi
[docs-rs]: https://docs.rs/tsukuyomi
[rust-version]: https://www.rust-lang.org
[master-doc]: https://tsukuyomi-rs.github.io/tsukuyomi
[gitter]: https://gitter.im/ubnt-intrepid/tsukuyomi
[examples]: https://github.com/tsukuyomi-rs/examples
[deps-rs]: https://deps.rs/crate/tsukuyomi/0.3.0
[travis]: https://travis-ci.org/tsukuyomi-rs/tsukuyomi
[azure-pipelines]: https://dev.azure.com/tsukuyomi-rs/tsukuyomi-rs/_build/latest?definitionId=1
[circleci]: https://circleci.com/gh/tsukuyomi-rs/tsukuyomi/tree/master
[codecov]: https://codecov.io/gh/tsukuyomi-rs/tsukuyomi

[crates-io-badge]: https://img.shields.io/crates/v/tsukuyomi.svg
[downloads-badge]: https://img.shields.io/crates/d/tsukuyomi.svg
[rust-version-badge]: https://img.shields.io/badge/rustc-1.30.0-lightgray.svg
[docs-rs-badge]: https://docs.rs/tsukuyomi/badge.svg
[master-doc-badge]: https://img.shields.io/badge/doc-master-blue.svg
[gitter-badge]: https://badges.gitter.im/ubnt-intrepid/tsukuyomi.svg
[deps-rs-badge]: https://deps.rs/crate/tsukuyomi/0.3.0/status.svg
[travis-badge]: https://travis-ci.org/tsukuyomi-rs/tsukuyomi.svg?branch=master
[azure-pipelines-badge]: https://dev.azure.com/tsukuyomi-rs/tsukuyomi-rs/_apis/build/status/tsukuyomi-rs.tsukuyomi
[circleci-badge]: https://circleci.com/gh/tsukuyomi-rs/tsukuyomi/tree/master.svg?style=svg
[codecov-badge]: https://codecov.io/gh/tsukuyomi-rs/tsukuyomi/branch/master/graph/badge.svg
