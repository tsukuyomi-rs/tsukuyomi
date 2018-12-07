use {
    failure::Error,
    std::ops::{Index, IndexMut},
};

#[derive(Debug, Copy, Clone, PartialEq, Eq)]
pub(super) struct NodeId {
    inner: NodeIdInner,
}

#[derive(Debug, Copy, Clone, PartialEq, Eq)]
enum NodeIdInner {
    Root,
    Index(usize),
}

impl NodeId {
    pub(super) fn root() -> Self {
        NodeId {
            inner: NodeIdInner::Root,
        }
    }
}

#[derive(Debug)]
pub(super) struct Arena<T> {
    nodes: Vec<Node<T>>,
    root: Node<T>,
}

impl<T> Arena<T> {
    pub(super) fn new(data: T) -> Self {
        Self {
            root: Node {
                id: NodeId::root(),
                parents: vec![],
                data,
            },
            nodes: vec![],
        }
    }

    pub(super) fn add_node(&mut self, parent: NodeId, data: T) -> Result<NodeId, Error> {
        let id = NodeId {
            inner: NodeIdInner::Index(self.nodes.len()),
        };

        let parent = &self[parent];

        let mut parents = parent.parents.clone();
        parents.push(parent.id);

        self.nodes.push(Node { id, parents, data });

        Ok(id)
    }
}

impl<T> Index<NodeId> for Arena<T> {
    type Output = Node<T>;

    fn index(&self, id: NodeId) -> &Self::Output {
        match id.inner {
            NodeIdInner::Root => &self.root,
            NodeIdInner::Index(i) => &self.nodes[i],
        }
    }
}

impl<T> IndexMut<NodeId> for Arena<T> {
    fn index_mut(&mut self, id: NodeId) -> &mut Self::Output {
        match id.inner {
            NodeIdInner::Root => &mut self.root,
            NodeIdInner::Index(i) => &mut self.nodes[i],
        }
    }
}

#[derive(Debug)]
pub(super) struct Node<T> {
    id: NodeId,
    parents: Vec<NodeId>,
    data: T,
}

impl<T> Node<T> {
    pub(super) fn id(&self) -> NodeId {
        self.id
    }

    pub(super) fn data(&self) -> &T {
        &self.data
    }
}
