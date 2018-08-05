use super::captures::CaptureNames;
use failure::Error;
use std::fmt;
use std::hash::{Hash, Hasher};
use std::str::FromStr;

/// A helper trait representing the conversion into an `Uri`.
pub(crate) trait TryIntoUri {
    type Error: Into<Error>;
    fn try_into(self) -> Result<Uri, Self::Error>;
}

impl TryIntoUri for Uri {
    type Error = Error;

    fn try_into(self) -> Result<Uri, Self::Error> {
        Ok(self)
    }
}

impl<'a> TryIntoUri for &'a str {
    type Error = Error;

    fn try_into(self) -> Result<Uri, Self::Error> {
        self.parse()
    }
}

/// Concatenate a list of Uris to an Uri.
pub(crate) fn join_all<I>(segments: I) -> Result<Uri, Error>
where
    I: IntoIterator,
    I::Item: AsRef<Uri>,
{
    segments
        .into_iter()
        .fold(Ok(Uri::root()), |acc, uri| acc?.join(uri))
}

/// A type representing the URI of a route.
#[derive(Debug, Clone)]
pub(crate) struct Uri(UriKind);

#[derive(Debug, Clone, PartialEq)]
enum UriKind {
    Root,
    Segments(String, Option<CaptureNames>),
}

impl PartialEq for Uri {
    fn eq(&self, other: &Uri) -> bool {
        match (&self.0, &other.0) {
            (&UriKind::Root, &UriKind::Root) => true,
            (&UriKind::Segments(ref s, ..), &UriKind::Segments(ref o, ..)) if s == o => true,
            _ => false,
        }
    }
}

impl Eq for Uri {}

impl Hash for Uri {
    fn hash<H: Hasher>(&self, state: &mut H) {
        match self.0 {
            UriKind::Root => "/".hash(state),
            UriKind::Segments(ref s, ..) => s.hash(state),
        }
    }
}

impl FromStr for Uri {
    type Err = Error;

    fn from_str(mut s: &str) -> Result<Uri, Self::Err> {
        if !s.is_ascii() {
            bail!("The URI is not ASCII");
        }

        if !s.starts_with('/') {
            bail!("invalid URI")
        }

        if s == "/" {
            return Ok(Uri::root());
        }

        let mut has_trailing_slash = false;
        if s.ends_with('/') {
            has_trailing_slash = true;
            s = &s[..s.len() - 1];
        }

        let mut names: Option<CaptureNames> = None;
        for segment in s[1..].split('/') {
            if segment.is_empty() {
                bail!("empty segment");
            }
            if names.as_ref().map_or(false, |names| names.wildcard) {
                bail!("The wildcard parameter has already set.");
            }

            if segment
                .get(1..)
                .map_or(false, |s| s.bytes().any(|b| b == b':' || b == b'*'))
            {
                bail!("invalid character in a segment");
            }
            match segment.as_bytes()[0] {
                c @ b':' | c @ b'*' => {
                    let names = names.get_or_insert_with(Default::default);
                    match c {
                        b':' => names.append(&segment[1..])?,
                        b'*' => names.set_wildcard()?,
                        _ => unreachable!(),
                    }
                }
                _ => {}
            }
        }

        if has_trailing_slash {
            Ok(Uri::segments(format!("{}/", s), names))
        } else {
            Ok(Uri::segments(s, names))
        }
    }
}

impl Uri {
    pub(crate) fn root() -> Uri {
        Uri(UriKind::Root)
    }

    fn segments(s: impl Into<String>, names: Option<CaptureNames>) -> Uri {
        Uri(UriKind::Segments(s.into(), names))
    }

    #[cfg(test)]
    fn static_(s: impl Into<String>) -> Uri {
        Uri::segments(s, None)
    }

    #[cfg(test)]
    fn captured(s: impl Into<String>, names: CaptureNames) -> Uri {
        Uri(UriKind::Segments(s.into(), Some(names)))
    }

    pub(super) fn as_str(&self) -> &str {
        match self.0 {
            UriKind::Root => "/",
            UriKind::Segments(ref s, ..) => s.as_str(),
        }
    }

    pub(crate) fn capture_names(&self) -> Option<&CaptureNames> {
        match self.0 {
            UriKind::Segments(_, Some(ref names)) => Some(names),
            _ => None,
        }
    }

    fn join(self, other: impl AsRef<Uri>) -> Result<Uri, Error> {
        match self.0 {
            UriKind::Root => Ok(other.as_ref().clone()),
            UriKind::Segments(mut s, mut names) => match other.as_ref().0 {
                | UriKind::Root => Ok(Uri::segments(s, names)),
                | UriKind::Segments(ref o, ref onames) => {
                    s += if s.ends_with('/') {
                        o.trim_left_matches('/')
                    } else {
                        o
                    };
                    match (&mut names, onames) {
                        (&mut Some(ref mut names), &Some(ref onames)) => {
                            names.extend(onames.params.iter().cloned())?;
                            if onames.wildcard {
                                names.set_wildcard()?;
                            }
                        }
                        (ref mut names @ None, &Some(ref onames)) => {
                            **names = Some(onames.clone());
                        }
                        (_, &None) => {}
                    }
                    Ok(Uri::segments(s, names))
                }
            },
        }
    }
}

impl AsRef<Uri> for Uri {
    fn as_ref(&self) -> &Uri {
        self
    }
}

impl fmt::Display for Uri {
    fn fmt(&self, f: &mut fmt::Formatter) -> fmt::Result {
        f.write_str(self.as_str())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    macro_rules! t {
        (@case $name:ident, $input:expr, $expected:expr) => {
            #[test]
            fn $name() {
                assert_eq!($input.ok().map(|uri: Uri| uri.0), Some($expected.0));
            }
        };
        ($(
            $name:ident ($input:expr, $expected:expr);
        )*) => {$(
            t!(@case $name, $input, $expected);
        )*};
    }

    t! [
        parse_uri_root(
            "/".parse(),
            Uri::root()
        );
        parse_uri_static(
            "/path/to/lib".parse(),
            Uri::static_("/path/to/lib")
        );
        parse_uri_static_has_trailing_slash(
            "/path/to/lib/".parse(),
            Uri::static_("/path/to/lib/")
        );
        parse_uri_has_wildcard_params(
            "/api/v1/:param/*path".parse(),
            Uri::captured(
                "/api/v1/:param/*path",
                CaptureNames {
                    params: indexset!["param".into()],
                    wildcard: true,
                }
            )
        );
    ];

    #[test]
    fn parse_uri_failcase_empty() {
        assert!("".parse::<Uri>().is_err());
    }

    #[test]
    fn parse_uri_failcase_without_prefix_root() {
        assert!("foo/bar".parse::<Uri>().is_err());
    }

    #[test]
    fn parse_uri_failcase_duplicated_slashes() {
        assert!("//foo/bar/".parse::<Uri>().is_err());
        assert!("/foo//bar/".parse::<Uri>().is_err());
        assert!("/foo/bar//".parse::<Uri>().is_err());
    }

    #[test]
    fn parse_uri_failcase_invalid_wildcard_specifier_pos() {
        assert!("/pa:th".parse::<Uri>().is_err());
    }

    #[test]
    fn parse_uri_failcase_non_ascii() {
        // FIXME: allow non-ascii URIs with encoding
        assert!("/パス".parse::<Uri>().is_err());
    }

    #[test]
    fn parse_uri_failcase_duplicated_param_name() {
        assert!("/:id/:id".parse::<Uri>().is_err());
    }

    #[test]
    fn parse_uri_failcase_after_wildcard_name() {
        assert!("/path/to/*a/id".parse::<Uri>().is_err());
    }

    t! [
        join_roots(
            Uri::root().join(Uri::root()),
            Uri::root()
        );
        join_root_and_static(
            Uri::root().join(Uri::static_("/path/to")),
            Uri::static_("/path/to")
        );
        join_trailing_slash_before_root_1(
            Uri::static_("/path/to/").join(Uri::root()),
            Uri::static_("/path/to/")
        );
        join_trailing_slash_before_root_2(
            Uri::static_("/path/to").join(Uri::root()),
            Uri::static_("/path/to")
        );
        join_trailing_slash_before_static_1(
            Uri::static_("/path").join(Uri::static_("/to")),
            Uri::static_("/path/to")
        );
        join_trailing_slash_before_static_2(
            Uri::static_("/path/").join(Uri::static_("/to")),
            Uri::static_("/path/to")
        );
    ];
}
