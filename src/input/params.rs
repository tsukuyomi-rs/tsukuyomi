use std::ops::Index;

use recognizer::Captures;

#[allow(missing_docs)]
#[derive(Debug)]
pub struct Params<'input> {
    pub(super) path: &'input str,
    pub(super) captures: &'input Captures,
}

#[allow(missing_docs)]
impl<'input> Params<'input> {
    pub fn is_empty(&self) -> bool {
        self.captures.params.is_empty() && self.captures.wildcard.is_none()
    }

    pub fn get(&self, i: usize) -> Option<&str> {
        self.captures
            .params
            .get(i)
            .and_then(|&(s, e)| self.path.get(s..e))
    }

    pub fn get_wildcard(&self) -> Option<&str> {
        self.captures
            .wildcard
            .and_then(|(s, e)| self.path.get(s..e))
    }
}

impl<'input> Index<usize> for Params<'input> {
    type Output = str;

    fn index(&self, i: usize) -> &Self::Output {
        self.get(i).expect("Out of range")
    }
}
