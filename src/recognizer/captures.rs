use failure::Error;
use indexmap::set::IndexSet;

#[derive(Clone, Debug, Default, PartialEq)]
pub(crate) struct CaptureNames {
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
}

#[derive(Debug, Default, PartialEq)]
pub(crate) struct Captures {
    pub(crate) params: Vec<(usize, usize)>,
    pub(crate) wildcard: Option<(usize, usize)>,
}
