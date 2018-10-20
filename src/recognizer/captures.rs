use failure::Error;
use indexmap::set::IndexSet;
use std::ops::Index;

#[derive(Clone, Debug, Default, PartialEq)]
pub(crate) struct CaptureNames {
    pub(super) params: IndexSet<String>,
    pub(super) wildcard: bool,
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
}

#[derive(Debug, Default, PartialEq)]
pub(crate) struct Captures {
    pub(super) params: Vec<(usize, usize)>,
    pub(super) wildcard: Option<(usize, usize)>,
}

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
