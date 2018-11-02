//! Components for accessing extracted parameters from HTTP path.

use std::ops::Index;

use crate::recognizer::captures::{CaptureNames, Captures};

/// A proxy object for accessing extracted parameters.
#[derive(Debug)]
pub struct Params<'input> {
    path: &'input str,
    names: Option<&'input CaptureNames>,
    captures: Option<&'input Captures>,
}

impl<'input> Params<'input> {
    pub(crate) fn new(
        path: &'input str,
        names: Option<&'input CaptureNames>,
        captures: Option<&'input Captures>,
    ) -> Params<'input> {
        debug_assert_eq!(names.is_some(), captures.is_some());
        Params {
            path,
            names,
            captures,
        }
    }

    /// Returns `true` if the extracted paramater exists.
    pub fn is_empty(&self) -> bool {
        self.captures.map_or(true, |caps| {
            caps.params.is_empty() && caps.wildcard.is_none()
        })
    }

    /// Returns the value of `i`-th parameter, if exists.
    pub fn get(&self, i: usize) -> Option<&str> {
        let &(s, e) = self.captures?.params.get(i)?;
        self.path.get(s..e)
    }

    /// Returns the value of wildcard parameter, if exists.
    pub fn get_wildcard(&self) -> Option<&str> {
        let (s, e) = self.captures?.wildcard?;
        self.path.get(s..e)
    }

    /// Returns the value of parameter whose name is equal to `name`, if exists.
    pub fn name(&self, name: &str) -> Option<&str> {
        match name {
            "*" => self.get_wildcard(),
            name => self.get(self.names?.params.get_full(name)?.0),
        }
    }
}

impl<'input> Index<usize> for Params<'input> {
    type Output = str;

    fn index(&self, i: usize) -> &Self::Output {
        self.get(i).expect("Out of range")
    }
}

impl<'input, 'a> Index<&'a str> for Params<'input> {
    type Output = str;

    fn index(&self, name: &'a str) -> &Self::Output {
        self.name(name).expect("Out of range")
    }
}
