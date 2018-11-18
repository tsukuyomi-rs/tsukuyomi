use {
    failure::Error,
    indexmap::IndexSet,
    std::{
        fmt,
        hash::{Hash, Hasher},
        str::FromStr,
    },
};

/// A helper trait representing the conversion into an `Uri`.
#[cfg_attr(feature = "cargo-clippy", allow(stutter))]
pub trait TryIntoUri {
    type Error: Into<Error>;

    fn try_into_uri(self) -> Result<Uri, Self::Error>;
}

impl TryIntoUri for Uri {
    type Error = Error;

    #[inline]
    fn try_into_uri(self) -> Result<Uri, Self::Error> {
        Ok(self)
    }
}

impl<'a> TryIntoUri for &'a str {
    type Error = Error;

    #[inline]
    fn try_into_uri(self) -> Result<Uri, Self::Error> {
        self.parse()
    }
}

/// Concatenate a list of Uris to an Uri.
pub fn join_all<I>(segments: I) -> Result<Uri, Error>
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
pub struct Uri(UriKind);

#[derive(Debug, Clone, PartialEq)]
enum UriKind {
    Root,
    Segments(String, Option<CaptureNames>),
}

impl PartialEq for Uri {
    fn eq(&self, other: &Self) -> bool {
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

    fn from_str(mut s: &str) -> Result<Self, Self::Err> {
        if !s.is_ascii() {
            failure::bail!("The URI is not ASCII");
        }

        if !s.starts_with('/') {
            failure::bail!("invalid URI")
        }

        if s == "/" {
            return Ok(Self::root());
        }

        let mut has_trailing_slash = false;
        if s.ends_with('/') {
            has_trailing_slash = true;
            s = &s[..s.len() - 1];
        }

        let mut names: Option<CaptureNames> = None;
        for segment in s[1..].split('/') {
            if segment.is_empty() {
                failure::bail!("empty segment");
            }
            if names.as_ref().map_or(false, |names| names.wildcard) {
                failure::bail!("The wildcard parameter has already set.");
            }

            if segment
                .get(1..)
                .map_or(false, |s| s.bytes().any(|b| b == b':' || b == b'*'))
            {
                failure::bail!("invalid character in a segment");
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
            Ok(Self::segments(format!("{}/", s), names))
        } else {
            Ok(Self::segments(s, names))
        }
    }
}

impl Uri {
    pub fn root() -> Self {
        Uri(UriKind::Root)
    }

    fn segments(s: impl Into<String>, names: Option<CaptureNames>) -> Self {
        Uri(UriKind::Segments(s.into(), names))
    }

    #[cfg(test)]
    fn static_(s: impl Into<String>) -> Self {
        Self::segments(s, None)
    }

    #[cfg(test)]
    fn captured(s: impl Into<String>, names: CaptureNames) -> Self {
        Uri(UriKind::Segments(s.into(), Some(names)))
    }

    pub fn as_str(&self) -> &str {
        match self.0 {
            UriKind::Root => "/",
            UriKind::Segments(ref s, ..) => s.as_str(),
        }
    }

    pub fn capture_names(&self) -> Option<&CaptureNames> {
        match self.0 {
            UriKind::Segments(_, Some(ref names)) => Some(names),
            _ => None,
        }
    }

    fn join(self, other: impl AsRef<Self>) -> Result<Self, Error> {
        match self.0 {
            UriKind::Root => Ok(other.as_ref().clone()),
            UriKind::Segments(mut segment, mut names) => match other.as_ref().0 {
                UriKind::Root => Ok(Self::segments(segment, names)),
                UriKind::Segments(ref other_segment, ref other_names) => {
                    segment += if segment.ends_with('/') {
                        other_segment.trim_left_matches('/')
                    } else {
                        other_segment
                    };
                    match (&mut names, other_names) {
                        (&mut Some(ref mut names), &Some(ref other_names)) => {
                            names.extend(other_names.params.iter().cloned())?;
                            if other_names.wildcard {
                                names.set_wildcard()?;
                            }
                        }
                        (ref mut names @ None, &Some(ref other_names)) => {
                            **names = Some(other_names.clone());
                        }
                        (_, &None) => {}
                    }
                    Ok(Self::segments(segment, names))
                }
            },
        }
    }
}

impl AsRef<Uri> for Uri {
    fn as_ref(&self) -> &Self {
        self
    }
}

impl fmt::Display for Uri {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(self.as_str())
    }
}

#[derive(Clone, Debug, Default, PartialEq)]
pub struct CaptureNames {
    pub(crate) params: IndexSet<String>,
    pub(crate) wildcard: bool,
}

impl CaptureNames {
    pub(super) fn append(&mut self, name: impl Into<String>) -> Result<(), Error> {
        if self.wildcard {
            failure::bail!("The wildcard parameter has already set");
        }

        let name = name.into();
        if name.is_empty() {
            failure::bail!("empty parameter name");
        }
        if !self.params.insert(name) {
            failure::bail!("the duplicated parameter name");
        }
        Ok(())
    }

    pub(super) fn extend<T>(&mut self, names: impl IntoIterator<Item = T>) -> Result<(), Error>
    where
        T: Into<String>,
    {
        for name in names {
            self.append(name)?;
        }
        Ok(())
    }

    pub(super) fn set_wildcard(&mut self) -> Result<(), Error> {
        if self.wildcard {
            failure::bail!("The wildcard parameter has already set");
        }
        self.wildcard = true;
        Ok(())
    }

    pub fn get_position(&self, name: &str) -> Option<usize> {
        Some(self.params.get_full(name)?.0)
    }
}

#[cfg_attr(feature = "cargo-clippy", allow(non_ascii_literal))]
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
