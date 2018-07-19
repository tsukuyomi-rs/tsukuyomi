use std::ops::Index;

#[allow(missing_docs)]
#[derive(Debug)]
pub struct Params<'input> {
    pub(super) path: &'input str,
    pub(super) params: Option<&'input [(usize, usize)]>,
}

#[allow(missing_docs)]
impl<'input> Params<'input> {
    pub fn is_empty(&self) -> bool {
        self.params.as_ref().map_or(true, |p| p.is_empty())
    }

    pub fn len(&self) -> usize {
        self.params.as_ref().map_or(0, |p| p.len())
    }

    pub fn get(&self, i: usize) -> Option<&str> {
        self.params
            .as_ref()
            .and_then(|p| p.get(i).and_then(|&(s, e)| self.path.get(s..e)))
    }
}

impl<'input> Index<usize> for Params<'input> {
    type Output = str;

    fn index(&self, i: usize) -> &Self::Output {
        self.get(i).expect("Out of range")
    }
}
