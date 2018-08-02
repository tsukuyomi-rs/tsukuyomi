#[derive(Debug, Default, PartialEq)]
pub(crate) struct Captures {
    pub(crate) params: Vec<(usize, usize)>,
    pub(crate) wildcard: Option<(usize, usize)>,
}
