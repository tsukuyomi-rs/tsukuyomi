#![cfg(unix)]

use std;
use tokio;

use super::imp::TransportImpl;
use super::Transport;

impl Transport for std::path::PathBuf {}
impl TransportImpl for std::path::PathBuf {
    type Item = tokio::net::UnixStream;
    type Error = std::io::Error;
    type Incoming = tokio::net::unix::Incoming;

    #[inline]
    fn incoming(self) -> std::io::Result<Self::Incoming> {
        (&self).incoming()
    }
}

impl<'a> Transport for &'a std::path::PathBuf {}
impl<'a> TransportImpl for &'a std::path::PathBuf {
    type Item = tokio::net::UnixStream;
    type Error = std::io::Error;
    type Incoming = tokio::net::unix::Incoming;

    #[inline]
    fn incoming(self) -> std::io::Result<Self::Incoming> {
        <&'a std::path::Path>::incoming(&*self)
    }
}

impl<'a> Transport for &'a std::path::Path {}
impl<'a> TransportImpl for &'a std::path::Path {
    type Item = tokio::net::UnixStream;
    type Error = std::io::Error;
    type Incoming = tokio::net::unix::Incoming;

    #[inline]
    fn incoming(self) -> std::io::Result<Self::Incoming> {
        Ok(tokio::net::UnixListener::bind(self)?.incoming())
    }
}
