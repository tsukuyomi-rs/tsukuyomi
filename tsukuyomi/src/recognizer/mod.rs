//! The implementation of route recognizer.

#[path = "tests_recognize.rs"]
mod tests;
mod tree;

use self::tree::Tree;
use crate::uri::{TryIntoUri, Uri};
use failure::Error;

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

        let index = self.uris.len();
        self.tree.insert(uri.as_str(), index)?;
        self.uris.push(uri);
        Ok(())
    }

    /// Traverses the given path and returns a reference to registered value of "T" if matched.
    ///
    /// At the same time, this method returns a sequence of pairs which indicates the range of
    /// substrings extracted as parameters.
    pub fn recognize(&self, path: &str) -> Option<(usize, Option<Captures>)> {
        self.tree.recognize(path)
    }
}
