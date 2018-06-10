use failure;
use futures::Future;
use http::Method;
use std::fmt;

use context::Context;
use error::Error;
use output::Output;
use router::Handler;

pub struct Route {
    pub(super) base: String,
    pub(super) path: String,
    method: Method,
    handler: Box<Fn(&Context) -> Box<Future<Item = Output, Error = Error> + Send> + Send + Sync + 'static>,
}

impl fmt::Debug for Route {
    fn fmt(&self, f: &mut fmt::Formatter) -> fmt::Result {
        f.debug_struct("Route")
            .field("path", &self.path)
            .field("method", &self.method)
            .finish()
    }
}

impl Route {
    pub fn new<H>(path: &str, method: Method, handler: H) -> Route
    where
        H: Handler + Send + Sync + 'static,
        H::Future: Send + 'static,
    {
        Route {
            base: String::new(),
            path: path.to_owned(),
            method: method,
            handler: Box::new(move |cx| {
                // TODO: specialization for Result<T, E>
                Box::new(handler.handle(cx))
            }),
        }
    }

    pub fn base(&self) -> &str {
        &self.base
    }

    pub fn path(&self) -> &str {
        &self.path
    }

    pub fn full_path(&self) -> String {
        join_uri(&self.base, &self.path)
    }

    pub fn method(&self) -> &Method {
        &self.method
    }

    pub(crate) fn handle(&self, cx: &Context) -> Box<Future<Item = Output, Error = Error> + Send> {
        (*self.handler)(cx)
    }
}

fn join_uri(base: &str, path: &str) -> String {
    format!("{}{}", base.trim_right_matches("/"), path)
}

pub(crate) fn normalize_uri(mut s: &str) -> Result<String, failure::Error> {
    if !s.is_ascii() {
        bail!("The URI is not ASCII");
    }

    if !s.starts_with("/") {
        bail!("invalid URI")
    }

    if s == "/" {
        return Ok("/".into());
    }

    let mut has_trailing_slash = false;
    if s.ends_with("/") {
        has_trailing_slash = true;
        s = &s[..s.len() - 1];
    }

    for segment in s[1..].split("/") {
        if segment.is_empty() {
            bail!("empty segment");
        }
        match segment.as_bytes()[0] {
            b':' | b'*' if segment.len() == 1 => bail!("empty parameter name"),
            _ => {}
        }
        if segment[1..].bytes().any(|b| b == b':' || b == b'*') {
            bail!("invalid character in a segment");
        }
    }

    if has_trailing_slash {
        Ok(format!("{}/", s))
    } else {
        Ok(s.into())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn normalize_uri_case_1() {
        assert_eq!(normalize_uri("/").ok(), Some("/".into()));
    }

    #[test]
    fn normalize_uri_case_2() {
        assert_eq!(normalize_uri("/path/to/lib").ok(), Some("/path/to/lib".into()));
    }

    #[test]
    fn normalize_uri_case_3() {
        assert_eq!(normalize_uri("/path/to/lib/").ok(), Some("/path/to/lib/".into()));
    }

    #[test]
    fn normalize_uri_case_4() {
        assert_eq!(
            normalize_uri("/api/v1/:param/*param").ok(),
            Some("/api/v1/:param/*param".into())
        );
    }

    #[test]
    fn normalize_uri_failcase_1() {
        assert!(normalize_uri("").is_err());
    }

    #[test]
    fn normalize_uri_failcase_2() {
        assert!(normalize_uri("foo/bar").is_err());
    }

    #[test]
    fn normalize_uri_failcase_3() {
        assert!(normalize_uri("/foo/bar//").is_err());
    }

    #[test]
    fn normalize_uri_failcase_4() {
        assert!(normalize_uri("/pa:th").is_err());
    }

    #[test]
    fn normalize_uri_failcase_5() {
        assert!(normalize_uri("/パス").is_err());
    }

    #[test]
    fn join_path_case1() {
        assert_eq!(join_uri("/", "/"), "/");
    }

    #[test]
    fn join_path_case2() {
        assert_eq!(join_uri("/path", "/to"), "/path/to");
    }

    #[test]
    fn join_path_case3() {
        assert_eq!(join_uri("/", "/path/to"), "/path/to");
    }

    #[test]
    fn join_path_case4() {
        assert_eq!(join_uri("/path/to/", "/"), "/path/to/");
    }
}
