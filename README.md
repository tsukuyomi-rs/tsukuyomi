<img src="https://raw.githubusercontent.com/tsukuyomi-rs/tsukuyomi/0.2/tsukuyomi-header.png" alt="header" width="500" />

---

[![Crates.io](https://img.shields.io/crates/v/tsukuyomi.svg)](https://crates.io/crates/tsukuyomi)
[![Crates.io (Downloads)](https://img.shields.io/crates/d/tsukuyomi.svg)](https://crates.io/crates/tsukuyomi)
[![Docs.rs](https://docs.rs/tsukuyomi/badge.svg)](https://docs.rs/tsukuyomi)
[![Gitter](https://badges.gitter.im/ubnt-intrepid/tsukuyomi.svg)](https://gitter.im/ubnt-intrepid/tsukuyomi?utm_source=badge&utm_medium=badge&utm_campaign=pr-badge)

Tsukuyomi is a next generation Web framework for Rust.

The ultimate goal of this project is to provide a Web framework for developing the asynchronous
and fast Web services, with the help of ecosystem of Rust for asynchronous network services like Tokio and Hyper.

## Features

* Supports HTTP/1.x and HTTP/2.0 protocols, based on Hyper 0.12
* Basic support for HTTP/1.1 protocol upgrade
* TLS support by using `rustls`
* Support for both TCP and Unix domain socket
* Custom error handling
* Basic support for Cookie management
* Middleware support

The following features does not currently implemented but will be supported in the future version:

* Custom session storage
* Authentication
* Embedded WebSocket handling

## Documentation

* [Examples](https://github.com/tsukuyomi-rs/examples)
* [API documentation (released)](https://docs.rs/tsukuyomi/0.2/tsukuyomi)
* [API documentation (master)](https://tsukuyomi-rs.github.io/tsukuyomi/tsukuyomi/index.html)

## Build Status

| Travis CI | Appveor | Coveralls |
|:---------:|:-------:|:---------:|
| [![Build Status][travis-badge]][travis] | [![Build status][appveyor-badge]][appveyor] | [![Coverage Status][coveralls-badge]][coveralls] |

[travis]: https://travis-ci.org/tsukuyomi-rs/tsukuyomi
[travis-badge]: https://travis-ci.org/tsukuyomi-rs/tsukuyomi.svg?branch=master
[appveyor]: https://ci.appveyor.com/project/ubnt-intrepid/tsukuyomi/branch/master
[appveyor-badge]: https://ci.appveyor.com/api/projects/status/kf8mx9k8iqfa08oj/branch/master?svg=true
[coveralls]: https://coveralls.io/github/tsukuyomi-rs/tsukuyomi?branch=master
[coveralls-badge]: https://coveralls.io/repos/github/tsukuyomi-rs/tsukuyomi/badge.svg?branch=master

## License
Tsukuyomi is licensed under either of [MIT license](LICENSE-MIT) or [Apache License, Version 2.0](LICENSE-APACHE) at your option.
