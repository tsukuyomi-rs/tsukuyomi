#[path = "tests_node.rs"]
mod tests;

use failure::Error;
use std::{cmp, fmt, mem};

use super::captures::Captures;

enum ChildKind {
    Segment,
    Param,
    Wildcard,
}

#[derive(PartialEq)]
pub(super) struct Node {
    path: Vec<u8>,
    leaf: Option<usize>,
    children: Vec<Node>,
}

#[cfg_attr(tarpaulin, skip)]
impl fmt::Debug for Node {
    fn fmt(&self, f: &mut fmt::Formatter) -> fmt::Result {
        f.debug_struct("Node")
            .field("path", &String::from_utf8_lossy(&self.path))
            .field("leaf", &self.leaf)
            .field("children", &self.children)
            .finish()
    }
}

impl Node {
    pub(super) fn new<S: Into<Vec<u8>>>(path: S) -> Node {
        Node {
            path: path.into(),
            leaf: None,
            children: vec![],
        }
    }

    pub(super) fn add_path(&mut self, path: &[u8], value: usize) -> Result<(), Error> {
        let mut n = self;
        let mut offset = 0;

        'walk: loop {
            if !n.is_wildcard() {
                // Find the longest common prefix
                let i = lcp(&path[offset..], &n.path[..]);

                // split the current segment and create a new node.
                if i < n.path.len() {
                    n.split_edge(i);
                }

                offset += i;
                if offset == path.len() {
                    break 'walk;
                }
            }

            // Insert the remaing path into the set of children.
            let c = path.get(offset);
            match c {
                Some(b':') | Some(b'*') => {
                    if n.children.iter().any(|ch| !ch.is_wildcard()) {
                        bail!("A static node has already inserted at wildcard position.");
                    }

                    if n.children.is_empty() {
                        if let Some(b'*') = c {
                            bail!("'catch-all' conflict");
                        }
                        n.insert_child(&path[offset..], value)?;
                        return Ok(());
                    }

                    n = &mut { n }.children[0];

                    // Find the end position of wildcard segment.
                    let end = find_wildcard_end(path, offset)?;
                    if path[offset..end] != n.path[..] {
                        bail!("wildcard conflict");
                    }
                    if end == path.len() {
                        break 'walk;
                    }
                    offset = end;
                }
                Some(&c) => {
                    if n.children.iter().any(|ch| ch.is_wildcard()) {
                        bail!("A wildcard node has already inserted.");
                    }

                    // Check if a child with the next path byte exists
                    for pos in 0..n.children.len() {
                        if n.children[pos].path[0] == c {
                            n = &mut { n }.children[pos];
                            continue 'walk;
                        }
                    }

                    // Otherwise, insert a new child node from remaining path.
                    let pos = find_wildcard_begin(path, offset);
                    let mut ch = Node {
                        path: path[offset..pos].to_owned(),
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

        if n.children.iter().any(|ch| ch.path.starts_with(b"*")) {
            bail!("catch-all conflict");
        }

        if n.leaf.is_some() {
            bail!("normal path conflict");
        }
        n.leaf = Some(value);

        Ok(())
    }

    pub(super) fn insert_child(&mut self, path: &[u8], value: usize) -> Result<(), Error> {
        let mut pos = 0;
        let mut n = self;

        while pos < path.len() {
            // Insert a wildcard node
            let i = find_wildcard_end(path, pos)?;
            n = { n }.add_child(&path[pos..i])?;
            pos = i;

            // Insert a normal node
            if pos < path.len() {
                let i = find_wildcard_begin(path, pos);
                n = { n }.add_child(&path[pos..i])?;
                pos = i;
            }
        }

        if n.leaf.is_some() {
            bail!("normal path conflict");
        }
        n.leaf = Some(value);

        Ok(())
    }

    pub(super) fn get_value<'r, 'p>(&'r self, path: &'p [u8]) -> Option<(usize, Captures)> {
        let mut offset = 0;
        let mut n = self;
        let mut captures = Captures::default();

        'walk: loop {
            if offset + n.path.len() >= path.len() {
                if n.path[..] != path[offset..] {
                    return None;
                }
                return match (n.leaf, n.children.get(0)) {
                    (Some(i), _) => Some((i, captures)),
                    (None, Some(ch)) if ch.path.get(0) == Some(&b'*') => {
                        captures.wildcard = Some((path.len(), path.len()));
                        Some((ch.leaf?, captures))
                    }
                    _ => None,
                };
            }

            if path[offset..offset + n.path.len()] != n.path[..] {
                return None;
            }

            offset += n.path.len();

            let (child, kind) = n.find_child(path, offset)?;
            n = child;
            match kind {
                ChildKind::Segment => {}
                ChildKind::Param => {
                    let span = path[offset..]
                        .into_iter()
                        .position(|&b| b == b'/')
                        .unwrap_or(path.len() - offset);
                    captures.params.push((offset, offset + span));
                    offset += span;
                    if offset >= path.len() {
                        return Some((n.leaf?, captures));
                    }

                    if n.children.is_empty() {
                        println!("[debug] d");
                        return None;
                    }
                    n = &n.children[0];
                }
                ChildKind::Wildcard => {
                    captures.wildcard = Some((offset, path.len()));
                    return Some((n.leaf?, captures));
                }
            }
        }
    }

    fn add_child<S: Into<Vec<u8>>>(&mut self, path: S) -> Result<&mut Node, Error> {
        let ch = Node {
            path: path.into(),
            leaf: None,
            children: vec![],
        };
        self.children.push(ch);
        Ok(self.children.iter_mut().last().unwrap())
    }

    fn split_edge(&mut self, i: usize) {
        let child = Node {
            path: self.path[i..].to_owned(),
            leaf: self.leaf.take(),
            children: mem::replace(&mut self.children, vec![]),
        };
        self.path = self.path[..i].into();
        self.children.push(child);
    }

    fn is_wildcard(&self) -> bool {
        match self.path.get(0) {
            Some(&b':') | Some(&b'*') => true,
            _ => false,
        }
    }

    fn find_child(&self, path: &[u8], offset: usize) -> Option<(&Node, ChildKind)> {
        let pred = path[offset];
        for ch in &self.children {
            match ch.path.get(0)? {
                b':' => return Some((ch, ChildKind::Param)),
                b'*' => return Some((ch, ChildKind::Wildcard)),
                &c if c == pred => return Some((ch, ChildKind::Segment)),
                _ => (),
            }
        }
        None
    }
}

/// Calculate the endpoint of longest common prefix between the two slices.
fn lcp(s1: &[u8], s2: &[u8]) -> usize {
    s1.into_iter()
        .zip(s2.into_iter())
        .position(|(s1, s2)| s1 != s2)
        .unwrap_or_else(|| cmp::min(s1.len(), s2.len()))
}

pub(super) fn find_wildcard_begin(path: &[u8], offset: usize) -> usize {
    path.into_iter()
        .skip(offset)
        .position(|&b| b == b':' || b == b'*')
        .map(|i| i + offset)
        .unwrap_or_else(|| path.len())
}

fn find_wildcard_end(path: &[u8], offset: usize) -> Result<usize, Error> {
    debug_assert!(path[offset] == b':' || path[offset] == b'*');
    if offset > 0 && path[offset - 1] != b'/' {
        bail!("a wildcard character (':' or '*') must be located at the next of slash");
    }
    let mut end = 1;
    while offset + end < path.len() && path[offset + end] != b'/' {
        match path[offset + end] {
            b':' | b'*' => bail!("wrong wildcard character (':' or '*') in a path segment"),
            _ => end += 1,
        }
    }
    if end == 1 {
        bail!("empty wildcard name");
    }
    if path[offset] == b'*' && offset + end < path.len() {
        bail!("a 'catch-all' param must be located at the end of path");
    }
    Ok(offset + end)
}
