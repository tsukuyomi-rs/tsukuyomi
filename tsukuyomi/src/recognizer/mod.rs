//! The implementation of route recognizer.

mod tree;

use {
    self::tree::Tree,
    crate::uri::{TryIntoUri, Uri},
    failure::Error,
};

#[derive(Debug, Default, PartialEq)]
pub struct Captures {
    params: Vec<(usize, usize)>,
    wildcard: Option<(usize, usize)>,
}

impl Captures {
    pub fn params(&self) -> &Vec<(usize, usize)> {
        &self.params
    }

    pub fn wildcard(&self) -> Option<(usize, usize)> {
        self.wildcard
    }
}

/// A route recognizer.
#[derive(Debug, Default)]
pub struct Recognizer {
    tree: Tree,
    asterisk: Option<usize>,
    uris: Vec<Uri>,
}

impl Recognizer {
    /// Add a path to this builder with a value of `T`.
    pub fn add_route<T>(&mut self, uri: T) -> Result<(), Error>
    where
        T: TryIntoUri,
    {
        let uri = uri.try_into_uri().map_err(Into::<Error>::into)?;
        if !uri.as_str().is_ascii() {
            failure::bail!("The path must be a sequence of ASCII characters");
        }

        if uri.is_asterisk() {
            if self.asterisk.is_some() {
                failure::bail!("the asterisk URI has already set");
            }
            self.asterisk = Some(self.uris.len());
        } else {
            self.tree.insert(uri.as_str(), self.uris.len())?;
        }

        self.uris.push(uri);
        Ok(())
    }

    /// Traverses the given path and returns a reference to registered value of "T" if matched.
    ///
    /// At the same time, this method returns a sequence of pairs which indicates the range of
    /// substrings extracted as parameters.
    pub fn recognize(&self, path: &str) -> Option<(usize, Option<Captures>)> {
        if path == "*" {
            self.asterisk.map(|pos| (pos, None))
        } else {
            self.tree.recognize(path)
        }
    }
}

#[cfg(test)]
mod tests {
    use super::{Captures, Recognizer};

    #[test]
    fn case1_empty() {
        let mut recognizer = Recognizer::default();
        recognizer.add_route("/").unwrap();

        assert_eq!(recognizer.recognize("/"), Some((0, None,)));
    }

    #[test]
    fn case2_multi_param() {
        let mut recognizer = Recognizer::default();
        recognizer.add_route("/files/:name/:id").unwrap();

        assert_eq!(
            recognizer.recognize("/files/readme/0"),
            Some((
                0,
                Some(Captures {
                    params: vec![(7, 13), (14, 15)],
                    wildcard: None,
                })
            ))
        );
    }

    #[test]
    fn case3_wildcard_root() {
        let mut recognizer = Recognizer::default();
        recognizer.add_route("/*path").unwrap();

        assert_eq!(
            recognizer.recognize("/path/to/readme.txt"),
            Some((
                0,
                Some(Captures {
                    params: vec![],
                    wildcard: Some((1, 19)),
                })
            ))
        );
    }

    #[test]
    fn case4_wildcard_subdir() {
        let mut recognizer = Recognizer::default();
        recognizer.add_route("/path/to/*path").unwrap();

        assert_eq!(
            recognizer.recognize("/path/to/readme.txt"),
            Some((
                0,
                Some(Captures {
                    params: vec![],
                    wildcard: Some((9, 19)),
                })
            ))
        );
    }

    #[test]
    fn case5_wildcard_empty_root() {
        let mut recognizer = Recognizer::default();
        recognizer.add_route("/*path").unwrap();

        assert_eq!(
            recognizer.recognize("/"),
            Some((
                0,
                Some(Captures {
                    params: vec![],
                    wildcard: Some((1, 1)),
                })
            ))
        );
    }

    #[test]
    fn case6_wildcard_empty_subdir() {
        let mut recognizer = Recognizer::default();
        recognizer.add_route("/path/to/*path").unwrap();

        assert_eq!(
            recognizer.recognize("/path/to/"),
            Some((
                0,
                Some(Captures {
                    params: vec![],
                    wildcard: Some((9, 9)),
                })
            ))
        );
    }

    #[test]
    fn case7_wildcard_empty_with_param() {
        let mut recognizer = Recognizer::default();
        recognizer.add_route("/path/to/:id/*path").unwrap();

        assert_eq!(
            recognizer.recognize("/path/to/10/"),
            Some((
                0,
                Some(Captures {
                    params: vec![(9, 11)],
                    wildcard: Some((12, 12)),
                })
            ))
        );
    }
}
