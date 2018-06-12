# Tsukuyomi

[![Build Status](https://travis-ci.org/ubnt-intrepid/tsukuyomi.svg?branch=master)](https://travis-ci.org/ubnt-intrepid/tsukuyomi)
[![Build status](https://ci.appveyor.com/api/projects/status/kf8mx9k8iqfa08oj/branch/master?svg=true)](https://ci.appveyor.com/project/ubnt-intrepid/tsukuyomi/branch/master)
[![Coverage Status](https://coveralls.io/repos/github/ubnt-intrepid/tsukuyomi/badge.svg?branch=master)](https://coveralls.io/github/ubnt-intrepid/tsukuyomi?branch=master)

Tsukuyomi is a next generation Web framework for Rust.

## The Goal of This Project

The ultimate goal of this project is to provide a Web framework for developing the asynchronous
and fast Web services, with the help of ecosystem of Rust for asynchronous network services like Tokio and Hyper.

## Features

* Supports HTTP/1.x and HTTP/2.0 protocols, based on Hyper 0.12
* Basic support for HTTP/1.1 protocol upgrade
* TLS support by using `rustls`
* Support for both TCP and Unix domain socket
* Custom error handling
* Basic support for Cookie management

The following features does not currently implemented but will be supported in the future version:

* Middlewares
* High-level APIs
  - Authentication
  - Session
  - WebSocket

## Documentation

* [API documentation (master)](https://ubnt-intrepid.github.io/tsukuyomi/tsukuyomi/index.html)

## License
MIT + Apache 2.0
