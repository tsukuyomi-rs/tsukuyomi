# `Tsukuyomi`

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

## Getting Started

Tsukuyomi requires the latest version of Rust compiler.
The required version of minimal Rust toolchain is 1.27.

You also need a nightly toolchain if you want to work with `futures-await` or upcoming async/await syntax.

```toml
[dependencies]
tsukuyomi = { git = "https://github.com/ubnt-intrepid/tsukuyomi.git" }

# It will be available from crates.io after releasing the next minor version.
# tsukuyomi = "0.2"

# ... and more dependencies
futures = "0.1.21"  # NOTE: DO NOT use 0.2.*
http = "0.1.6"
```

```rust
extern crate tsukuyomi;
extern crate futures;

use tsukuyomi::{App, Input, Error};
use futures::Future;

// The definition of *synchronous* handler.
// It will return a `Responder` which immediately convert into an HTTP response,
// and does not need any asynchronous computation.
fn handler(_input: &mut Input) -> &'static str {
    "Hello, Tsukuyomi.\n"
}

// The definition of *asynchronous* handler.
// It will return a `Future` representing the remaining computation in the handler.
fn async_handler(input: &mut Input)
    -> impl Future<Item = String, Error = Error> + Send + 'static
{
    input.body_mut().read_all().convert_to::<String>()
        .and_then(|body| {
            Ok(format!("Received: {}", body))
        })
        .inspect(|_| {
            // You can access a mutable reference to `Input` stored in
            // the task-local storage by using Input::with_get():
            Input::with_get(|input| {
                println!("[debug] path = {}", input.uri().path());
            })
        })
}

fn main() -> tsukuyomi::AppResult<()> {
    let app = App::builder()
        .mount("/", |m| {
            m.get("/").handle(handler);
            m.get("/async").handle_async_with_input(async_handler);
        })
        .finish()?;

    tsukuyomi::run(app);
}
```

More examples are located in [`examples/`](examples/).

If you want to experiment with these examples, try to clone this repository and run the following command:

```shell-session
$ cargo run -p example-basic
```

## Documentation

* [API documentation (released)](https://docs.rs/tsukuyomi/*/tsukuyomi)
* [API documentation (master)](https://ubnt-intrepid.github.io/tsukuyomi/tsukuyomi/index.html)

## Build Status

| Travis CI | Appveor | Coveralls |
|:---------:|:-------:|:---------:|
| [![Build Status][travis-badge]][travis] | [![Build status][appveyor-badge]][appveyor] | [![Coverage Status][coveralls-badge]][coveralls] |

[travis]: https://travis-ci.org/ubnt-intrepid/tsukuyomi
[travis-badge]: https://travis-ci.org/ubnt-intrepid/tsukuyomi.svg?branch=master
[appveyor]: https://ci.appveyor.com/project/ubnt-intrepid/tsukuyomi/branch/master
[appveyor-badge]: https://ci.appveyor.com/api/projects/status/kf8mx9k8iqfa08oj/branch/master?svg=true
[coveralls]: https://coveralls.io/github/ubnt-intrepid/tsukuyomi?branch=master
[coveralls-badge]: https://coveralls.io/repos/github/ubnt-intrepid/tsukuyomi/badge.svg?branch=master

## License
Tsukuyomi is licensed under either of [MIT license](LICENSE-MIT) or [Apache License, Version 2.0](LICENSE-APACHE) at your option.
