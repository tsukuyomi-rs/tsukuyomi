//! The implementation of route recognizer.

use {
    failure::Error,
    indexmap::{indexset, IndexSet},
    std::{
        cmp::{self, Ordering},
        fmt, mem,
    },
};

#[derive(Debug, Default, PartialEq)]
pub struct Captures {
    params: Vec<(usize, usize)>,
    wildcard: Option<(usize, usize)>,
}

impl Captures {
    pub fn params(&self) -> &Vec<(usize, usize)> {
        &self.params
    }

    pub fn wildcard(&self) -> Option<(usize, usize)> {
        self.wildcard
    }
}

/// A route recognizer.
#[derive(Debug, Default)]
pub struct Recognizer {
    paths: Vec<String>,
    tree: Tree,
    asterisk: Option<usize>,
}

impl Recognizer {
    /// Add a path to this builder with a value of `T`.
    pub fn add_path(&mut self, path: &str) -> Result<(), Error> {
        if !path.is_ascii() {
            failure::bail!("The path must be a sequence of ASCII characters");
        }

        if path == "*" {
            if self.asterisk.is_some() {
                failure::bail!("the asterisk URI has already set");
            }
            self.asterisk = Some(self.paths.len());
        } else {
            self.tree.insert(path.as_ref(), self.paths.len())?;
        }

        self.paths.push(path.into());

        Ok(())
    }

    /// Traverses the given path and returns a reference to registered value of "T" if matched.
    ///
    /// At the same time, this method returns a sequence of pairs which indicates the range of
    /// substrings extracted as parameters.
    pub fn recognize(&self, path: &str) -> Option<(usize, Option<Captures>)> {
        if path == "*" {
            self.asterisk.map(|pos| (pos, None))
        } else {
            self.tree.recognize(path.as_ref())
        }
    }
}

#[derive(Clone, PartialEq)]
enum NodeKind {
    Static(Vec<u8>),
    Param,
    CatchAll,
}

#[cfg_attr(tarpaulin, skip)]
impl fmt::Debug for NodeKind {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            NodeKind::Static(ref s) => f
                .debug_tuple("Static")
                .field(&String::from_utf8_lossy(s))
                .finish(),
            NodeKind::Param => f.debug_tuple("Param").finish(),
            NodeKind::CatchAll => f.debug_tuple("CatchAll").finish(),
        }
    }
}

#[derive(Debug, PartialEq)]
struct Node {
    kind: NodeKind,
    candidates: IndexSet<usize>,
    leaf: Option<usize>,
    children: Vec<Node>,
}

impl Node {
    fn add_path(&mut self, path: &[u8], value: usize) -> Result<(), Error> {
        let mut n = self;
        let mut offset = 0;

        'walk: loop {
            let pos = if let NodeKind::Static(ref s) = n.kind {
                // Find the longest common prefix
                Some((longest_common_prefix(&path[offset..], &s[..]), s.len()))
            } else {
                None
            };
            if let Some((i, s_len)) = pos {
                // split the current segment and create a new node.
                if i < s_len {
                    let (p1, p2) = match n.kind {
                        NodeKind::Static(ref s) => (s[..i].to_owned(), s[i..].to_owned()),
                        _ => panic!("unexpected condition"),
                    };
                    let child = Self {
                        kind: NodeKind::Static(p2),
                        candidates: n.candidates.clone(),
                        leaf: n.leaf.take(),
                        children: mem::replace(&mut n.children, vec![]),
                    };
                    n.kind = NodeKind::Static(p1);
                    n.children.push(child);
                }

                n.candidates.insert(value);

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
                    if n.children.iter().any(|ch| match ch.kind {
                        NodeKind::Static(..) => true,
                        _ => false,
                    }) {
                        failure::bail!("A static node has already inserted at wildcard position.");
                    }

                    n.candidates.insert(value);
                    n = &mut { n }.children[0];
                    let end = find_wildcard_end(path, offset)?;
                    if end == path.len() {
                        break 'walk;
                    }
                    offset = end;
                }

                Some(&c) => {
                    // Check if a child with the next path byte exists
                    let mut ch_pos = None;
                    for (i, ch) in n.children.iter().enumerate() {
                        match ch.kind {
                            NodeKind::Static(ref s) => {
                                if s[0] == c {
                                    ch_pos = Some(i);
                                    break;
                                }
                            }
                            NodeKind::Param | NodeKind::CatchAll => {
                                failure::bail!("A wildcard node has already inserted.")
                            }
                        }
                    }
                    if let Some(pos) = ch_pos {
                        n.candidates.insert(value);
                        n = &mut { n }.children[pos];
                        continue 'walk;
                    }

                    // Otherwise, insert a new child node from remaining path.
                    let pos = find_wildcard_begin(path, offset);
                    let mut ch = Self {
                        kind: NodeKind::Static(path[offset..pos].to_owned()),
                        candidates: indexset![value],
                        leaf: None,
                        children: vec![],
                    };
                    ch.insert_child(&path[pos..], value)?;
                    n.children.push(ch);
                    n.candidates.insert(value);

                    return Ok(());
                }
                _ => unreachable!(),
            }
        }

        if n.children.iter().any(|ch| ch.kind == NodeKind::CatchAll) {
            failure::bail!("catch-all conflict");
        }

        n.set_leaf(value)?;
        n.candidates.insert(value);
        Ok(())
    }

    fn insert_child(&mut self, path: &[u8], value: usize) -> Result<(), Error> {
        let mut pos = 0;
        let mut n = self;

        while pos < path.len() {
            // Insert a wildcard node
            let i = find_wildcard_end(path, pos)?;
            n.children.push(Self {
                kind: match path[pos] {
                    b':' => NodeKind::Param,
                    b'*' => NodeKind::CatchAll,
                    c => failure::bail!("unexpected parameter type: '{}'", c),
                },
                candidates: indexset![value],
                leaf: None,
                children: vec![],
            });
            n = { n }.children.iter_mut().last().unwrap();
            pos = i;

            // Insert a normal node
            if pos < path.len() {
                let index = find_wildcard_begin(path, pos);
                n.children.push(Self {
                    kind: NodeKind::Static(path[pos..index].into()),
                    leaf: None,
                    candidates: indexset![value],
                    children: vec![],
                });
                n = { n }.children.iter_mut().last().unwrap();
                pos = index;
            }
        }

        n.set_leaf(value).expect("the leaf should be empty");
        Ok(())
    }

    fn get_value(&self, path: &[u8], captures: &mut Option<Captures>) -> Option<usize> {
        let mut offset = 0;
        let mut n = self;

        loop {
            log::trace!("n = {:?}", n);
            log::trace!("captures = {:?}", captures);
            log::trace!("offset = {}", offset);

            match n.kind {
                NodeKind::Static(ref s) => match compare_length(&s[..], &path[offset..]) {
                    Ordering::Less if path[offset..].starts_with(&s[..]) => offset += s.len(),
                    Ordering::Greater if s[..].starts_with(&path[offset..]) => offset = path.len(),
                    Ordering::Equal if s[..] == path[offset..] => {
                        offset = path.len();
                        if let Some(i) = n.leaf {
                            return Some(i);
                        }
                    }
                    _ => return None,
                },
                NodeKind::Param => {
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
                }
                NodeKind::CatchAll => {
                    captures.get_or_insert_with(Default::default).wildcard =
                        Some((offset, path.len()));
                    return n.leaf;
                }
            }

            n = n.children.iter().find(|ch| match ch.kind {
                NodeKind::Static(ref s) => s[0] == path[offset],
                NodeKind::Param | NodeKind::CatchAll => true,
            })?;
        }
    }

    fn set_leaf(&mut self, value: usize) -> Result<(), Error> {
        if self.leaf.is_some() {
            failure::bail!("normal path conflict");
        }
        self.leaf = Some(value);
        Ok(())
    }
}

#[derive(Debug, Default)]
struct Tree {
    root: Option<Node>,
}

impl Tree {
    fn insert(&mut self, path: &[u8], index: usize) -> Result<(), Error> {
        if let Some(ref mut root) = self.root {
            root.add_path(path, index)?;
            return Ok(());
        }

        let pos = find_wildcard_begin(path, 0);
        self.root
            .get_or_insert(Node {
                kind: NodeKind::Static(path[..pos].into()),
                candidates: indexset![index],
                leaf: None,
                children: vec![],
            }).insert_child(&path[pos..], index)?;

        Ok(())
    }

    fn recognize(&self, path: &[u8]) -> Option<(usize, Option<Captures>)> {
        let mut captures = None;
        let i = self
            .root
            .as_ref()?
            .get_value(path.as_ref(), &mut captures)?;
        Some((i, captures))
    }
}

/// Calculate the endpoint of longest common prefix between the two slices.
fn longest_common_prefix(s1: &[u8], s2: &[u8]) -> usize {
    s1.into_iter()
        .zip(s2.into_iter())
        .position(|(s1, s2)| s1 != s2)
        .unwrap_or_else(|| cmp::min(s1.len(), s2.len()))
}

fn compare_length<T>(s1: &[T], s2: &[T]) -> Ordering {
    s1.len().cmp(&s2.len())
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

#[cfg(test)]
mod tests {
    use super::{Captures, Recognizer};

    #[test]
    fn case1_empty() {
        let mut recognizer = Recognizer::default();
        recognizer.add_path("/").unwrap();

        assert_eq!(recognizer.recognize("/"), Some((0, None,)));
    }

    #[test]
    fn case2_multi_param() {
        let mut recognizer = Recognizer::default();
        recognizer.add_path("/files/:name/:id").unwrap();

        assert_eq!(
            recognizer.recognize("/files/readme/0"),
            Some((
                0,
                Some(Captures {
                    params: vec![(7, 13), (14, 15)],
                    wildcard: None,
                })
            ))
        );
    }

    #[test]
    fn case3_wildcard_root() {
        let mut recognizer = Recognizer::default();
        recognizer.add_path("/*path").unwrap();

        assert_eq!(
            recognizer.recognize("/path/to/readme.txt"),
            Some((
                0,
                Some(Captures {
                    params: vec![],
                    wildcard: Some((1, 19)),
                })
            ))
        );
    }

    #[test]
    fn case4_wildcard_subdir() {
        let mut recognizer = Recognizer::default();
        recognizer.add_path("/path/to/*path").unwrap();

        assert_eq!(
            recognizer.recognize("/path/to/readme.txt"),
            Some((
                0,
                Some(Captures {
                    params: vec![],
                    wildcard: Some((9, 19)),
                })
            ))
        );
    }

    #[test]
    fn case5_wildcard_empty_root() {
        let mut recognizer = Recognizer::default();
        recognizer.add_path("/*path").unwrap();

        assert_eq!(
            recognizer.recognize("/"),
            Some((
                0,
                Some(Captures {
                    params: vec![],
                    wildcard: Some((1, 1)),
                })
            ))
        );
    }

    #[test]
    fn case6_wildcard_empty_subdir() {
        let mut recognizer = Recognizer::default();
        recognizer.add_path("/path/to/*path").unwrap();

        assert_eq!(
            recognizer.recognize("/path/to/"),
            Some((
                0,
                Some(Captures {
                    params: vec![],
                    wildcard: Some((9, 9)),
                })
            ))
        );
    }

    #[test]
    fn case7_wildcard_empty_with_param() {
        let mut recognizer = Recognizer::default();
        recognizer.add_path("/path/to/:id/*path").unwrap();

        assert_eq!(
            recognizer.recognize("/path/to/10/"),
            Some((
                0,
                Some(Captures {
                    params: vec![(9, 11)],
                    wildcard: Some((12, 12)),
                })
            ))
        );
    }
}

#[cfg(test)]
mod tests_tree {
    use {
        super::{Node, NodeKind, Tree},
        indexmap::indexset,
    };

    macro_rules! t {
        ($test:ident, [$($path:expr),*], $expected:expr) => {
            #[test]
            fn $test() {
                let mut tree = Tree::default();
                for (i, path) in [$($path),*].iter().enumerate() {
                    tree.insert(path.as_bytes(), i).unwrap();
                }
                assert_eq!(tree.root, Some($expected));
            }
        };
        ($test:ident, [$($path:expr,)+], $expected:expr) => {
            t!($test, [$($path),*], $expected);
        };
    }

    #[test]
    fn case0() {
        let tree = Tree::default();
        assert_eq!(tree.root, None);
    }

    t!(
        case1,
        ["/foo"],
        Node {
            kind: NodeKind::Static("/foo".into()),
            candidates: indexset![0],
            leaf: Some(0),
            children: vec![],
        }
    );

    t!(
        case2,
        ["/foo", "/bar"],
        Node {
            kind: NodeKind::Static("/".into()),
            candidates: indexset![0, 1],
            leaf: None,
            children: vec![
                Node {
                    kind: NodeKind::Static("foo".into()),
                    candidates: indexset![0],
                    leaf: Some(0),
                    children: vec![],
                },
                Node {
                    kind: NodeKind::Static("bar".into()),
                    candidates: indexset![1],
                    leaf: Some(1),
                    children: vec![],
                },
            ],
        }
    );

    t!(
        case3,
        ["/foo", "/foobar"],
        Node {
            kind: NodeKind::Static("/foo".into()),
            candidates: indexset![0, 1],
            leaf: Some(0),
            children: vec![Node {
                kind: NodeKind::Static("bar".into()),
                candidates: indexset![1],
                leaf: Some(1),
                children: vec![],
            }],
        }
    );

    t!(
        param_case1,
        ["/:id"],
        Node {
            kind: NodeKind::Static("/".into()),
            leaf: None,
            candidates: indexset![0],
            children: vec![Node {
                kind: NodeKind::Param, // ":id"
                leaf: Some(0),
                candidates: indexset![0],
                children: vec![],
            }],
        }
    );

    t!(
        param_case2,
        [
            "/files",
            "/files/:name/likes/",
            "/files/:name",
            "/files/:name/likes/:id/",
            "/files/:name/likes/:id",
        ],
        Node {
            kind: NodeKind::Static("/files".into()),
            leaf: Some(0),
            candidates: indexset![0, 1, 2, 3, 4],
            children: vec![Node {
                kind: NodeKind::Static("/".into()),
                leaf: None,
                candidates: indexset![1, 2, 3, 4],
                children: vec![Node {
                    kind: NodeKind::Param, // ":name"
                    leaf: Some(2),
                    candidates: indexset![1, 2, 3, 4],
                    children: vec![Node {
                        kind: NodeKind::Static("/likes/".into()),
                        leaf: Some(1),
                        candidates: indexset![1, 3, 4],
                        children: vec![Node {
                            kind: NodeKind::Param, // ":id"
                            leaf: Some(4),
                            candidates: indexset![3, 4],
                            children: vec![Node {
                                kind: NodeKind::Static("/".into()),
                                leaf: Some(3),
                                candidates: indexset![3],
                                children: vec![],
                            }],
                        }],
                    }],
                }],
            }],
        }
    );

    t!(
        catch_all_case1,
        ["/*path"],
        Node {
            kind: NodeKind::Static("/".into()),
            leaf: None,
            candidates: indexset![0],
            children: vec![Node {
                kind: NodeKind::CatchAll, // "*path"
                leaf: Some(0),
                candidates: indexset![0],
                children: vec![],
            }],
        }
    );

    t!(
        catch_all_case2,
        ["/files", "/files/*path"],
        Node {
            kind: NodeKind::Static("/files".into()),
            leaf: Some(0),
            candidates: indexset![0, 1],
            children: vec![Node {
                kind: NodeKind::Static("/".into()),
                leaf: None,
                candidates: indexset![1],
                children: vec![Node {
                    kind: NodeKind::CatchAll, // "*path"
                    leaf: Some(1),
                    candidates: indexset![1],
                    children: vec![],
                }],
            }],
        }
    );

    #[test]
    fn failcase1() {
        let mut tree = Tree::default();
        assert!(tree.insert(b"/foo", 0).is_ok());
        assert!(tree.insert(b"/:id", 1).is_err());
    }

    #[test]
    fn failcase2() {
        let mut tree = Tree::default();
        assert!(tree.insert(b"/foo/", 0).is_ok());
        assert!(tree.insert(b"/foo/*path", 1).is_err());
    }

    #[test]
    fn failcase3() {
        let mut tree = Tree::default();
        assert!(tree.insert(b"/:id", 0).is_ok());
        assert!(tree.insert(b"/foo", 1).is_err());
    }

    #[test]
    fn failcase4() {
        let mut tree = Tree::default();
        assert!(tree.insert(b"/foo/*path", 0).is_ok());
        assert!(tree.insert(b"/foo/", 1).is_err());
    }

    #[test]
    fn failcase5() {
        let mut tree = Tree::default();
        assert!(tree.insert(b"/:id", 0).is_ok());
        assert!(tree.insert(b"/:name", 1).is_err());
    }

    #[test]
    fn failcase6() {
        let mut tree = Tree::default();
        assert!(tree.insert(b"/:id", 0).is_ok());
        assert!(tree.insert(b"/*id", 1).is_err());
    }

    #[test]
    fn failcase7() {
        let mut tree = Tree::default();
        assert!(tree.insert(b"/*id", 0).is_ok());
        assert!(tree.insert(b"/:id", 1).is_err());
    }

    #[test]
    fn failcase8() {
        let mut tree = Tree::default();
        assert!(tree.insert(b"/path/to", 0).is_ok());
        assert!(tree.insert(b"/path/to", 1).is_err());
    }
}
