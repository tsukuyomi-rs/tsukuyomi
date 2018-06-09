# Ganymede

[![Build Status](https://travis-ci.org/ubnt-intrepid/ganymede.svg?branch=master)](https://travis-ci.org/ubnt-intrepid/ganymede)
[![Build status](https://ci.appveyor.com/api/projects/status/uadxfu3y6jh768a0/branch/master?svg=true)](https://ci.appveyor.com/project/ubnt-intrepid/ganymede/branch/master)
[![Coverage Status](https://coveralls.io/repos/github/ubnt-intrepid/ganymede/badge.svg?branch=master)](https://coveralls.io/github/ubnt-intrepid/ganymede?branch=master)

Ganymede is a next generation Web framework for Rust.  
This project is the successor of Susanoo.

**WARNING: This project is now actively development.**

## The Goal of This Project

The ultimate goal of this project is to provide a Web framework for developing the asynchronous
and fast Web services, with the help of ecosystem of Rust for asynchronous network services like Tokio and Hyper.

## Features

* Supports HTTP/1.x and HTTP/2.0 protocols, based on Hyper 0.12
* Basic support for HTTP/1.1 protocol upgrade
* TLS support by using `rustls`
* Support for both TCP and Unix domain socket
* Custom error handling
* Cookies

The following features will be supported:

* Middlewares
* High-level API for WebSocket handling

## Documentation

* [API documentation (master)](https://ubnt-intrepid.github.io/ganymede/ganymede/index.html)

## License
MIT + Apache 2.0
