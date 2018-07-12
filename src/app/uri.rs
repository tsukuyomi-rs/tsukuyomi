#![allow(missing_docs)]

use failure::Error;
use std::fmt;

pub(super) fn join_all<I>(prefix: I) -> Uri
where
    I: IntoIterator,
    I::Item: AsRef<Uri>,
{
    let mut uri = String::new();
    for p in prefix {
        if p.as_ref().0 != "/" {
            uri = format!("{}{}", uri.trim_right_matches("/"), p.as_ref());
        }
    }

    if uri.is_empty() {
        Uri::new()
    } else {
        Uri(uri)
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Hash)]
pub struct Uri(String);

impl Uri {
    pub fn new() -> Uri {
        Uri("/".into())
    }

    pub fn from_str(s: &str) -> Result<Uri, Error> {
        normalize_uri(s).map(Uri)
    }
}

impl AsRef<Uri> for Uri {
    fn as_ref(&self) -> &Uri {
        self
    }
}

impl AsRef<str> for Uri {
    fn as_ref(&self) -> &str {
        self.0.as_ref()
    }
}

impl fmt::Display for Uri {
    fn fmt(&self, f: &mut fmt::Formatter) -> fmt::Result {
        f.write_str(&self.0)
    }
}

fn normalize_uri(mut s: &str) -> Result<String, Error> {
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
        assert_eq!(join_all(&[Uri("/".into()), Uri("/".into())]), Uri("/".into()));
    }

    #[test]
    fn join_path_case2() {
        assert_eq!(
            join_all(&[Uri("/path".into()), Uri("/to".into())]),
            Uri("/path/to".into())
        );
    }

    #[test]
    fn join_path_case3() {
        assert_eq!(
            join_all(&[Uri("/path/".into()), Uri("/to".into())]),
            Uri("/path/to".into())
        );
    }

    #[test]
    fn join_path_case4() {
        assert_eq!(
            join_all(&[Uri("/".into()), Uri("/path/to".into())]),
            Uri("/path/to".into())
        );
    }

    #[test]
    fn join_path_case5() {
        assert_eq!(
            join_all(&[Uri("/path/to/".into()), Uri("/".into())]),
            Uri("/path/to/".into())
        );
    }

    #[test]
    fn join_path_case6() {
        assert_eq!(
            join_all(&[Uri("/path/to".into()), Uri("/".into())]),
            Uri("/path/to".into())
        );
    }
}
