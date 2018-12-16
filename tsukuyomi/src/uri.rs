use {
    crate::util::{Never, TryFrom},
    failure::Error,
    indexmap::{indexset, IndexSet},
    std::{
        fmt,
        hash::{Hash, Hasher},
        str::FromStr,
    },
};

#[derive(Debug, Clone, PartialEq)]
enum UriKind {
    Root,
    Segments(String, Option<CaptureNames>),
}

/// A type representing the URI of a route.
#[derive(Debug, Clone)]
pub struct Uri(UriKind);

impl Default for Uri {
    fn default() -> Self {
        Self::root()
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
        self.as_str().hash(state)
    }
}

impl FromStr for Uri {
    type Err = Error;

    fn from_str(s: &str) -> Result<Self, Self::Err> {
        Self::parse(s)
    }
}

impl TryFrom<Self> for Uri {
    type Error = Never;

    #[inline]
    fn try_from(uri: Self) -> Result<Self, Self::Error> {
        Ok(uri)
    }
}

impl<'a> TryFrom<&'a str> for Uri {
    type Error = failure::Error;

    #[inline]
    fn try_from(s: &'a str) -> Result<Self, Self::Error> {
        s.parse()
    }
}

impl TryFrom<String> for Uri {
    type Error = failure::Error;

    #[inline]
    fn try_from(s: String) -> Result<Self, Self::Error> {
        s.parse()
    }
}

impl AsRef<str> for Uri {
    fn as_ref(&self) -> &str {
        self.as_str()
    }
}

impl Uri {
    pub fn root() -> Self {
        Uri(UriKind::Root)
    }

    pub fn from_static(s: &'static str) -> Self {
        s.parse().expect("invalid URI")
    }

    pub fn parse(mut s: &str) -> Result<Self, Error> {
        if !s.is_ascii() {
            failure::bail!("The URI is not ASCII");
        }

        if !s.starts_with('/') {
            failure::bail!("the URI must start with '/'");
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
            if names.as_ref().map_or(false, |names| names.has_wildcard) {
                failure::bail!("The wildcard parameter has already set.");
            }
            if segment.is_empty() {
                failure::bail!("empty segment");
            }
            if segment
                .get(1..)
                .map_or(false, |s| s.bytes().any(|b| b == b':' || b == b'*'))
            {
                failure::bail!("invalid character in a segment");
            }
            match segment.as_bytes()[0] {
                b':' | b'*' => {
                    names.get_or_insert_with(Default::default).push(segment)?;
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

    pub fn push(&mut self, component: UriComponent) -> Result<(), Error> {
        self.0 = match self.0 {
            UriKind::Root => match component {
                UriComponent::Static(s) => UriKind::Segments(format!("/{}", s), None),
                UriComponent::Param(name, param_kind) => UriKind::Segments(
                    format!("/{}{}", param_kind, name),
                    Some(CaptureNames {
                        params: indexset! { name.to_string() },
                        has_wildcard: param_kind == '*',
                    }),
                ),
                UriComponent::Slash => UriKind::Root,
            },
            UriKind::Segments(ref mut segments, ref mut names) => match component {
                UriComponent::Static(s) => {
                    if names.as_ref().map_or(false, |names| names.has_wildcard) {
                        failure::bail!("the catch-all parameter has already registered");
                    }

                    if !segments.ends_with('/') {
                        segments.push('/');
                    }
                    segments.push_str(&s);

                    return Ok(());
                }
                UriComponent::Param(name, param_kind) => {
                    let names = names.get_or_insert_with(Default::default);
                    if names.params.contains(&name) {
                        failure::bail!("duplicated parameter name");
                    }
                    if names.has_wildcard {
                        failure::bail!("the catch-all parameter has already been registered");
                    }

                    if !segments.ends_with('/') {
                        segments.push('/');
                    }
                    segments.push(param_kind);
                    segments.push_str(&name);

                    names.params.insert(name);
                    names.has_wildcard = param_kind == '*';

                    return Ok(());
                }
                UriComponent::Slash => {
                    if names.as_ref().map_or(false, |names| names.has_wildcard) {
                        failure::bail!("the catch-all parameter has already registered");
                    }

                    if !segments.ends_with('/') {
                        segments.push('/');
                    }

                    return Ok(());
                }
            },
        };

        Ok(())
    }

    pub fn join(&self, other: impl AsRef<Self>) -> Result<Self, Error> {
        match self.0.clone() {
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

#[derive(Debug)]
pub enum UriComponent {
    Static(String),
    Param(String, char),
    Slash,
}

#[derive(Clone, Debug, Default, PartialEq)]
pub struct CaptureNames {
    params: IndexSet<String>,
    has_wildcard: bool,
}

impl CaptureNames {
    fn push(&mut self, segment: &str) -> Result<(), Error> {
        if self.has_wildcard {
            failure::bail!("The wildcard parameter has already set");
        }

        let (kind, name) = segment.split_at(1);
        match kind {
            ":" | "*" => {}
            "" => failure::bail!("empty segment"),
            c => failure::bail!("unknown parameter kind: '{}'", c),
        }

        if name.is_empty() {
            failure::bail!("empty parameter name");
        }

        if !self.params.insert(name.into()) {
            failure::bail!("the duplicated parameter name");
        }

        if kind == "*" {
            self.has_wildcard = true;
        }

        Ok(())
    }

    fn extend<T>(&mut self, names: impl IntoIterator<Item = T>) -> Result<(), Error>
    where
        T: AsRef<str>,
    {
        for name in names {
            self.push(name.as_ref())?;
        }
        Ok(())
    }

    pub fn position(&self, name: &str) -> Option<usize> {
        Some(self.params.get_full(name)?.0)
    }
}

#[allow(clippy::non_ascii_literal)]
#[cfg(test)]
mod tests {
    use {super::*, indexmap::indexset};

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
                    params: indexset!["param".into(), "path".into()],
                    has_wildcard: true,
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

    #[test]
    fn push_uri_1() -> Result<(), failure::Error> {
        let mut uri = Uri::root();
        uri.push(UriComponent::Static("path".into()))?;
        uri.push(UriComponent::Static("to".into()))?;
        uri.push(UriComponent::Param("id".into(), ':'))?;
        uri.push(UriComponent::Param("name".into(), ':'))?;
        uri.push(UriComponent::Param("path".into(), '*'))?;

        assert_eq!(uri.as_str(), "/path/to/:id/:name/*path");

        let names = uri.capture_names().expect("should be Some");
        assert_eq!(
            names.params,
            indexset! { "id".into(), "name".into(), "path".into() }
        );
        assert!(names.has_wildcard);

        Ok(())
    }

    #[test]
    fn push_uri_2() -> Result<(), failure::Error> {
        let mut uri = Uri::root();
        uri.push(UriComponent::Param("id".into(), ':'))?;

        assert_eq!(uri.as_str(), "/:id");

        let names = uri.capture_names().expect("should be Some");
        assert_eq!(names.params, indexset! { "id".into(), });
        assert!(!names.has_wildcard);

        Ok(())
    }

    #[test]
    fn push_uri_3() -> Result<(), failure::Error> {
        let mut uri = Uri::root();
        uri.push(UriComponent::Param("path".into(), '*'))?;

        assert_eq!(uri.as_str(), "/*path");

        let names = uri.capture_names().expect("should be Some");
        assert_eq!(names.params, indexset! { "path".into(), });
        assert!(names.has_wildcard);

        Ok(())
    }

    #[test]
    fn push_uri_4() -> Result<(), failure::Error> {
        let mut uri = Uri::root();
        uri.push(UriComponent::Static("path".into()))?;
        uri.push(UriComponent::Slash)?;

        assert_eq!(uri.as_str(), "/path/");
        assert!(uri.capture_names().is_none());

        Ok(())
    }

    #[test]
    fn push_uri_slash_after_catch_all() {
        let mut uri = Uri::try_from("/*path").unwrap();
        assert!(uri.push(UriComponent::Slash).is_err());
    }
}
