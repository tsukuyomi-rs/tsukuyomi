//! The implementation of route recognizer.

// NOTE: The original implementation was imported from https://github.com/ubnt-intrepid/susanoo

mod captures;
mod node;

pub(crate) use self::captures::Captures;
use self::node::{find_wildcard_begin, Node};

#[path = "tests_recognize.rs"]
mod tests;

use failure::Error;

/// A route recognizer.
#[derive(Debug, Default)]
pub(crate) struct Recognizer {
    root: Option<Node>,
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

        if let Some(ref mut root) = self.root {
            root.add_path(path.as_bytes(), index)?;
            self.paths.push(path);
            return Ok(());
        }

        let pos = find_wildcard_begin(path.as_bytes(), 0);
        self.root
            .get_or_insert(Node::new(&path[..pos]))
            .insert_child(path[pos..].as_bytes(), index)?;
        self.paths.push(path);

        Ok(())
    }

    /// Traverses the given path and returns a reference to registered value of "T" if matched.
    ///
    /// At the same time, this method returns a sequence of pairs which indicates the range of
    /// substrings extracted as parameters.
    pub(crate) fn recognize(&self, path: &str) -> Option<(usize, Captures)> {
        self.root.as_ref()?.get_value(path.as_bytes())
    }
}
