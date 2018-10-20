//! The implementation of route recognizer.

pub(crate) mod captures;
mod tree;
pub(crate) mod uri;

#[path = "tests_recognize.rs"]
mod tests;

use failure::Error;

use self::captures::Captures;
use self::tree::Tree;
use self::uri::{TryIntoUri, Uri};

/// A route recognizer.
#[derive(Debug, Default)]
pub(crate) struct Recognizer {
    tree: Tree,
    uris: Vec<Uri>,
}

impl Recognizer {
    /// Add a path to this builder with a value of `T`.
    pub(crate) fn add_route(&mut self, uri: impl TryIntoUri) -> Result<(), Error> {
        let uri = uri.try_into().map_err(Into::<Error>::into)?;

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
    pub(crate) fn recognize(&self, path: &str) -> Option<(usize, Option<Captures>)> {
        self.tree.recognize(path)
    }
}
