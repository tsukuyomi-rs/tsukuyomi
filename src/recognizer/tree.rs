#[path = "tests_tree.rs"]
mod tests;

use failure::Error;
use std::{cmp, fmt, mem};

use super::captures::Captures;

#[derive(Clone, PartialEq)]
enum PathKind {
    Segment(Vec<u8>),
    Param,
    CatchAll,
}

#[cfg_attr(tarpaulin, skip)]
impl fmt::Debug for PathKind {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            PathKind::Segment(ref s) => f
                .debug_tuple("Segment")
                .field(&String::from_utf8_lossy(s))
                .finish(),
            PathKind::Param => f.debug_tuple("Param").finish(),
            PathKind::CatchAll => f.debug_tuple("CatchAll").finish(),
        }
    }
}

impl PathKind {
    fn segment(path: impl Into<Vec<u8>>) -> Self {
        PathKind::Segment(path.into())
    }
}

#[derive(Debug, PartialEq)]
struct Node {
    path: PathKind,
    leaf: Option<usize>,
    children: Vec<Node>,
}

impl Node {
    fn new(path: PathKind) -> Self {
        Self {
            path,
            leaf: None,
            children: vec![],
        }
    }

    fn add_path(&mut self, path: &[u8], value: usize) -> Result<(), Error> {
        let mut n = self;
        let mut offset = 0;

        'walk: loop {
            let pos = if let PathKind::Segment(ref s) = n.path {
                // Find the longest common prefix
                Some((lcp(&path[offset..], &s[..]), s.len()))
            } else {
                None
            };
            if let Some((i, s_len)) = pos {
                // split the current segment and create a new node.
                if i < s_len {
                    n.split_edge(i);
                }

                offset += i;
                if offset == path.len() {
                    break 'walk;
                }
            }

            // Insert the remaing path into the set of children.
            match path.get(offset) {
                Some(b':') if n.children.is_empty() => {
                    n.insert_child(&path[offset..], value)?;
                    return Ok(());
                }

                Some(b'*') if n.children.is_empty() => failure::bail!("'catch-all' conflict"),

                Some(b':') | Some(b'*') => {
                    if n.children.iter().any(|ch| !ch.is_wildcard()) {
                        failure::bail!("A static node has already inserted at wildcard position.");
                    }

                    n = &mut { n }.children[0];
                    let end = find_wildcard_end(path, offset)?;
                    if end == path.len() {
                        break 'walk;
                    }
                    offset = end;
                }

                Some(&c) => {
                    if n.children.iter().any(|ch| ch.is_wildcard()) {
                        failure::bail!("A wildcard node has already inserted.");
                    }

                    // Check if a child with the next path byte exists
                    if let Some(pos) = n.find_child_position(c, true) {
                        n = &mut { n }.children[pos];
                        continue 'walk;
                    }

                    // Otherwise, insert a new child node from remaining path.
                    let pos = find_wildcard_begin(path, offset);
                    let mut ch = Self {
                        path: PathKind::Segment(path[offset..pos].to_owned()),
                        leaf: None,
                        children: vec![],
                    };
                    ch.insert_child(&path[pos..], value)?;
                    n.children.push(ch);

                    return Ok(());
                }
                _ => unreachable!(),
            }
        }

        if n.children.iter().any(|ch| ch.path == PathKind::CatchAll) {
            failure::bail!("catch-all conflict");
        }

        if n.leaf.is_some() {
            failure::bail!("normal path conflict");
        }
        n.leaf = Some(value);

        Ok(())
    }

    fn insert_child(&mut self, path: &[u8], value: usize) -> Result<(), Error> {
        let mut pos = 0;
        let mut n = self;

        while pos < path.len() {
            // Insert a wildcard node
            let i = find_wildcard_end(path, pos)?;
            let path_kind = match path[pos] {
                b':' => PathKind::Param,
                b'*' => PathKind::CatchAll,
                c => failure::bail!("unexpected parameter type: '{}'", c),
            };
            n = { n }.add_child(path_kind)?;
            pos = i;

            // Insert a normal node
            if pos < path.len() {
                let index = find_wildcard_begin(path, pos);
                n = { n }.add_child(PathKind::segment(&path[pos..index]))?;
                pos = index;
            }
        }

        if n.leaf.is_some() {
            failure::bail!("normal path conflict");
        }
        n.leaf = Some(value);

        Ok(())
    }

    fn get_value(&self, path: &[u8], captures: &mut Option<Captures>) -> Option<usize> {
        let mut offset = 0;
        let mut n = self;

        loop {
            if let PathKind::Segment(ref s) = n.path {
                if offset + s.len() >= path.len() {
                    if s[..] != path[offset..] {
                        return None;
                    }
                    return match (n.leaf, n.children.get(0)) {
                        (Some(i), _) => Some(i),
                        (None, Some(ch)) if ch.path == PathKind::CatchAll => {
                            captures.get_or_insert_with(Default::default).wildcard =
                                Some((path.len(), path.len()));
                            ch.leaf
                        }
                        _ => None,
                    };
                }

                if path[offset..offset + s.len()] != s[..] {
                    return None;
                }

                offset += s.len();
            } else {
                panic!("unexpected condition");
            }

            let pos = n.find_child_position(path[offset], false)?;
            n = &n.children[pos];
            match n.path {
                PathKind::Segment(..) => {}
                PathKind::Param => {
                    let span = path[offset..]
                        .into_iter()
                        .position(|&b| b == b'/')
                        .unwrap_or(path.len() - offset);
                    captures
                        .get_or_insert_with(Default::default)
                        .params
                        .push((offset, offset + span));
                    offset += span;
                    if offset >= path.len() {
                        return n.leaf;
                    }

                    if n.children.is_empty() {
                        return None;
                    }
                    n = &n.children[0];
                }
                PathKind::CatchAll => {
                    captures.get_or_insert_with(Default::default).wildcard =
                        Some((offset, path.len()));
                    return n.leaf;
                }
            }
        }
    }

    fn add_child(&mut self, path: PathKind) -> Result<&mut Self, Error> {
        self.children.push(Self::new(path));
        Ok(self.children.iter_mut().last().unwrap())
    }

    fn split_edge(&mut self, i: usize) {
        let (p1, p2) = match self.path {
            PathKind::Segment(ref s) => (s[..i].to_owned(), s[i..].to_owned()),
            _ => panic!("unexpected condition"),
        };
        let child = Self {
            path: PathKind::Segment(p2),
            leaf: self.leaf.take(),
            children: mem::replace(&mut self.children, vec![]),
        };
        self.path = PathKind::Segment(p1);
        self.children.push(child);
    }

    fn is_wildcard(&self) -> bool {
        match self.path {
            PathKind::Param | PathKind::CatchAll => true,
            _ => false,
        }
    }

    fn find_child_position(&self, c: u8, ignore_wildcard: bool) -> Option<usize> {
        for (pos, ch) in self.children.iter().enumerate() {
            match ch.path {
                PathKind::Segment(ref s) if s[0] == c => return Some(pos),
                PathKind::Param | PathKind::CatchAll if !ignore_wildcard => return Some(pos),
                _ => {}
            }
        }
        None
    }
}

#[derive(Debug, Default)]
pub(super) struct Tree {
    root: Option<Node>,
}

impl Tree {
    pub(super) fn insert(&mut self, path: impl AsRef<[u8]>, index: usize) -> Result<(), Error> {
        let path = path.as_ref();

        if let Some(ref mut root) = self.root {
            root.add_path(path, index)?;
            return Ok(());
        }

        let pos = find_wildcard_begin(path, 0);
        self.root
            .get_or_insert(Node::new(PathKind::Segment(path[..pos].into())))
            .insert_child(&path[pos..], index)?;

        Ok(())
    }

    pub(super) fn recognize(&self, path: impl AsRef<[u8]>) -> Option<(usize, Option<Captures>)> {
        let mut captures = None;
        let i = self
            .root
            .as_ref()?
            .get_value(path.as_ref(), &mut captures)?;
        Some((i, captures))
    }
}

/// Calculate the endpoint of longest common prefix between the two slices.
fn lcp(s1: &[u8], s2: &[u8]) -> usize {
    s1.into_iter()
        .zip(s2.into_iter())
        .position(|(s1, s2)| s1 != s2)
        .unwrap_or_else(|| cmp::min(s1.len(), s2.len()))
}

fn find_wildcard_begin(path: &[u8], offset: usize) -> usize {
    path.into_iter()
        .skip(offset)
        .position(|&b| b == b':' || b == b'*')
        .map_or_else(|| path.len(), |i| i + offset)
}

fn find_wildcard_end(path: &[u8], offset: usize) -> Result<usize, Error> {
    debug_assert!(path[offset] == b':' || path[offset] == b'*');
    if offset > 0 && path[offset - 1] != b'/' {
        failure::bail!("a wildcard character (':' or '*') must be located at the next of slash");
    }
    let mut end = 1;
    while offset + end < path.len() && path[offset + end] != b'/' {
        match path[offset + end] {
            b':' | b'*' => {
                failure::bail!("wrong wildcard character (':' or '*') in a path segment")
            }
            _ => end += 1,
        }
    }
    if end == 1 {
        failure::bail!("empty wildcard name");
    }
    if path[offset] == b'*' && offset + end < path.len() {
        failure::bail!("a 'catch-all' param must be located at the end of path");
    }
    Ok(offset + end)
}
