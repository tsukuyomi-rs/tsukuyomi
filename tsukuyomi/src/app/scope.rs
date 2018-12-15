use {
    failure::Error,
    std::ops::{Index, IndexMut},
};

#[derive(Debug, Copy, Clone, PartialEq, Eq)]
pub(super) struct ScopeId {
    inner: ScopeIdInner,
}

#[derive(Debug, Copy, Clone, PartialEq, Eq)]
enum ScopeIdInner {
    Root,
    Index(usize),
}

impl ScopeId {
    pub(super) fn root() -> Self {
        ScopeId {
            inner: ScopeIdInner::Root,
        }
    }
}

#[derive(Debug)]
pub(super) struct Scopes<T> {
    nodes: Vec<Scope<T>>,
    root: Scope<T>,
}

impl<T> Scopes<T> {
    pub(super) fn new(data: T) -> Self {
        Self {
            root: Scope {
                id: ScopeId::root(),
                ancestors: vec![],
                data,
            },
            nodes: vec![],
        }
    }

    pub(super) fn add_node(&mut self, parent: ScopeId, data: T) -> Result<ScopeId, Error> {
        let id = ScopeId {
            inner: ScopeIdInner::Index(self.nodes.len()),
        };

        let parent = &self[parent];

        let mut ancestors = parent.ancestors.clone();
        ancestors.push(parent.id);

        self.nodes.push(Scope {
            id,
            ancestors,
            data,
        });

        Ok(id)
    }
}

impl<T> Index<ScopeId> for Scopes<T> {
    type Output = Scope<T>;

    fn index(&self, id: ScopeId) -> &Self::Output {
        match id.inner {
            ScopeIdInner::Root => &self.root,
            ScopeIdInner::Index(i) => &self.nodes[i],
        }
    }
}

impl<T> IndexMut<ScopeId> for Scopes<T> {
    fn index_mut(&mut self, id: ScopeId) -> &mut Self::Output {
        match id.inner {
            ScopeIdInner::Root => &mut self.root,
            ScopeIdInner::Index(i) => &mut self.nodes[i],
        }
    }
}

#[derive(Debug)]
pub(super) struct Scope<T> {
    id: ScopeId,
    ancestors: Vec<ScopeId>,
    pub(super) data: T,
}

impl<T> Scope<T> {
    pub(super) fn id(&self) -> ScopeId {
        self.id
    }

    pub(super) fn ancestors(&self) -> &[ScopeId] {
        &self.ancestors[..]
    }
}
