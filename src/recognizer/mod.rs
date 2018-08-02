//! The implementation of route recognizer.

// NOTE: The original implementation was imported from https://github.com/ubnt-intrepid/susanoo

mod captures;
#[path = "tests_recognize.rs"]
mod tests;
mod tree;

use failure::Error;

pub(crate) use self::captures::Captures;
use self::tree::Tree;

/// A route recognizer.
#[derive(Debug, Default)]
pub(crate) struct Recognizer {
    tree: Tree,
    paths: Vec<String>,
}

impl Recognizer {
    /// Add a path to this builder with a value of `T`.
    pub(crate) fn add_route(&mut self, path: impl Into<String>) -> Result<(), Error> {
        let path = path.into();
        if !path.is_ascii() {
            bail!("The path must be a sequence of ASCII characters");
        }

        let index = self.paths.len();
        self.tree.insert(path.as_bytes(), index)?;
        self.paths.push(path);
        Ok(())
    }

    /// Traverses the given path and returns a reference to registered value of "T" if matched.
    ///
    /// At the same time, this method returns a sequence of pairs which indicates the range of
    /// substrings extracted as parameters.
    pub(crate) fn recognize(&self, path: &str) -> Option<(usize, Captures)> {
        self.tree.recognize(path.as_bytes())
    }
}
