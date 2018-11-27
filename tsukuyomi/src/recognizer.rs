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

#[derive(Clone, Debug, PartialEq)]
pub struct Candidates(IndexSet<usize>);

impl Candidates {
    fn insert(&mut self, value: usize) {
        self.0.insert(value);
    }

    pub(crate) fn iter<'a>(&'a self) -> impl Iterator<Item = usize> + 'a {
        self.0.iter().cloned()
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
            InsertContext {
                path: path.as_ref(),
                index: self.paths.len(),
            } //
            .visit_tree(&mut self.tree)?;
        }

        self.paths.push(path.into());

        Ok(())
    }

    /// Traverses the given path and returns a reference to registered value of "T" if matched.
    ///
    /// At the same time, this method returns a sequence of pairs which indicates the range of
    /// substrings extracted as parameters.
    pub fn recognize(&self, path: &str, captures: &mut Option<Captures>) -> Recognize<'_> {
        if path == "*" {
            self.asterisk.ok_or_else(|| RecognizeError::NotMatched)
        } else {
            RecognizeContext {
                path: path.as_ref(),
                captures,
            } //
            .visit_tree(&self.tree)
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
    candidates: Candidates,
    leaf: Option<usize>,
    children: Vec<Node>,
}

#[derive(Debug, Default)]
struct Tree {
    root: Option<Node>,
}

#[derive(Debug)]
struct InsertContext<'a> {
    path: &'a [u8],
    index: usize,
}

impl<'a> InsertContext<'a> {
    fn add_path(&self, mut n: &mut Node) -> Result<(), Error> {
        let mut offset = 0;
        'walk: loop {
            let pos = if let NodeKind::Static(ref s) = n.kind {
                // Find the longest common prefix
                Some((longest_common_prefix(&self.path[offset..], &s[..]), s.len()))
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
                    let child = Node {
                        kind: NodeKind::Static(p2),
                        candidates: n.candidates.clone(),
                        leaf: n.leaf.take(),
                        children: mem::replace(&mut n.children, vec![]),
                    };
                    n.kind = NodeKind::Static(p1);
                    n.children.push(child);
                }

                n.candidates.insert(self.index);

                offset += i;
                if offset == self.path.len() {
                    break 'walk;
                }
            }

            // Insert the remaing path into the set of children.
            match self.path.get(offset) {
                Some(b':') if n.children.is_empty() => {
                    self.insert_child(n, offset)?;
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

                    n.candidates.insert(self.index);
                    n = &mut { n }.children[0];
                    let end = find_wildcard_end(self.path, offset)?;
                    if end == self.path.len() {
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
                        n.candidates.insert(self.index);
                        n = &mut { n }.children[pos];
                        continue 'walk;
                    }

                    // Otherwise, insert a new child node from remaining path.
                    let pos = find_wildcard_begin(self.path, offset);
                    let mut ch = self.new_node(NodeKind::Static(self.path[offset..pos].to_owned()));
                    self.insert_child(&mut ch, pos)?;
                    n.children.push(ch);
                    n.candidates.insert(self.index);

                    return Ok(());
                }
                _ => unreachable!(),
            }
        }

        if n.children.iter().any(|ch| ch.kind == NodeKind::CatchAll) {
            failure::bail!("catch-all conflict");
        }

        self.set_leaf(n)?;
        n.candidates.insert(self.index);
        Ok(())
    }

    fn insert_child(&self, mut n: &mut Node, offset: usize) -> Result<(), Error> {
        let path = &self.path[offset..];
        let mut pos = 0;

        while pos < path.len() {
            // Insert a wildcard node
            let i = find_wildcard_end(path, pos)?;
            n.children.push(self.new_node(match path[pos] {
                b':' => NodeKind::Param,
                b'*' => NodeKind::CatchAll,
                c => failure::bail!("unexpected parameter type: '{}'", c),
            }));
            n = { n }.children.iter_mut().last().unwrap();
            pos = i;

            // Insert a normal node
            if pos < path.len() {
                let index = find_wildcard_begin(path, pos);
                n.children
                    .push(self.new_node(NodeKind::Static(path[pos..index].into())));
                n = { n }.children.iter_mut().last().unwrap();
                pos = index;
            }
        }

        self.set_leaf(n).expect("the leaf should be empty");
        Ok(())
    }

    fn set_leaf(&self, n: &mut Node) -> Result<(), Error> {
        if n.leaf.is_some() {
            failure::bail!("normal path conflict");
        }
        n.leaf = Some(self.index);
        Ok(())
    }

    fn new_node(&self, kind: NodeKind) -> Node {
        Node {
            kind,
            candidates: Candidates(indexset![self.index]),
            leaf: None,
            children: vec![],
        }
    }

    fn visit_tree(&self, tree: &mut Tree) -> Result<(), Error> {
        if let Some(ref mut root) = tree.root {
            return self.add_path(root);
        }

        let pos = find_wildcard_begin(self.path, 0);
        let root = tree
            .root
            .get_or_insert_with(|| self.new_node(NodeKind::Static(self.path[..pos].into())));
        self.insert_child(root, pos)?;

        Ok(())
    }
}

// ===== recognize =====

pub type Recognize<'a> = Result<usize, RecognizeError<'a>>;

#[derive(Debug, PartialEq)]
pub enum RecognizeError<'a> {
    NotMatched,
    PartiallyMatched(&'a Candidates),
}

#[derive(Debug)]
struct RecognizeContext<'a> {
    path: &'a [u8],
    captures: &'a mut Option<Captures>,
}

impl<'a> RecognizeContext<'a> {
    fn recognize<'t>(&mut self, mut n: &'t Node) -> Recognize<'t> {
        let mut offset = 0;
        loop {
            match n.kind {
                NodeKind::Static(ref s) => match compare_length(&s[..], &self.path[offset..]) {
                    Ordering::Less if self.path[offset..].starts_with(&s[..]) => offset += s.len(),
                    Ordering::Greater if s[..].starts_with(&self.path[offset..]) => {
                        offset = self.path.len()
                    }
                    Ordering::Equal if s[..] == self.path[offset..] => {
                        offset = self.path.len();
                        if let Some(i) = n.leaf {
                            return Ok(i);
                        }
                    }
                    _ => return Err(RecognizeError::NotMatched),
                },
                NodeKind::Param => {
                    let span = self.path[offset..]
                        .into_iter()
                        .position(|&b| b == b'/')
                        .unwrap_or(self.path.len() - offset);
                    self.captures
                        .get_or_insert_with(Default::default)
                        .params
                        .push((offset, offset + span));
                    offset += span;

                    if offset >= self.path.len() {
                        return n
                            .leaf //
                            .ok_or_else(|| RecognizeError::PartiallyMatched(&n.candidates));
                    }
                }
                NodeKind::CatchAll => {
                    self.captures.get_or_insert_with(Default::default).wildcard =
                        Some((offset, self.path.len()));
                    return n
                        .leaf //
                        .ok_or_else(|| RecognizeError::PartiallyMatched(&n.candidates));
                }
            }

            n = n
                .children
                .iter()
                .find(|ch| match ch.kind {
                    NodeKind::Static(ref s) => self.path.get(offset).map_or(false, |&c| s[0] == c),
                    NodeKind::Param | NodeKind::CatchAll => true,
                }) //
                .ok_or_else(|| RecognizeError::PartiallyMatched(&n.candidates))?;
        }
    }

    fn visit_tree<'t>(&mut self, tree: &'t Tree) -> Result<usize, RecognizeError<'t>> {
        let root = tree
            .root
            .as_ref()
            .ok_or_else(|| RecognizeError::NotMatched)?;
        self.recognize(root)
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
    use {
        super::{Candidates, Captures, RecognizeError, Recognizer},
        indexmap::indexset,
    };

    #[test]
    fn case1_empty() {
        let mut recognizer = Recognizer::default();
        recognizer.add_path("/").unwrap();

        let mut captures = None;
        assert_eq!(recognizer.recognize("/", &mut captures), Ok(0));
        assert_eq!(captures, None);
    }

    #[test]
    fn case2_multi_param() {
        let mut recognizer = Recognizer::default();
        recognizer.add_path("/files/:name/:id").unwrap();

        let mut captures = None;
        assert_eq!(
            recognizer.recognize("/files/readme/0", &mut captures),
            Ok(0)
        );
        assert_eq!(
            captures,
            Some(Captures {
                params: vec![(7, 13), (14, 15)],
                wildcard: None,
            })
        );
    }

    #[test]
    fn case3_wildcard_root() {
        let mut recognizer = Recognizer::default();
        recognizer.add_path("/*path").unwrap();

        let mut captures = None;
        assert_eq!(
            recognizer.recognize("/path/to/readme.txt", &mut captures),
            Ok(0)
        );
        assert_eq!(
            captures,
            Some(Captures {
                params: vec![],
                wildcard: Some((1, 19)),
            })
        );
    }

    #[test]
    fn case4_wildcard_subdir() {
        let mut recognizer = Recognizer::default();
        recognizer.add_path("/path/to/*path").unwrap();

        let mut captures = None;
        assert_eq!(
            recognizer.recognize("/path/to/readme.txt", &mut captures),
            Ok(0)
        );
        assert_eq!(
            captures,
            Some(Captures {
                params: vec![],
                wildcard: Some((9, 19)),
            })
        );
    }

    #[test]
    fn case5_wildcard_empty_root() {
        let mut recognizer = Recognizer::default();
        recognizer.add_path("/*path").unwrap();

        let mut captures = None;
        assert_eq!(recognizer.recognize("/", &mut captures), Ok(0));
        assert_eq!(
            captures,
            Some(Captures {
                params: vec![],
                wildcard: Some((1, 1)),
            })
        );
    }

    #[test]
    fn case6_wildcard_empty_subdir() {
        let mut recognizer = Recognizer::default();
        recognizer.add_path("/path/to/*path").unwrap();

        let mut captures = None;
        assert_eq!(recognizer.recognize("/path/to/", &mut captures), Ok(0));
        assert_eq!(
            captures,
            Some(Captures {
                params: vec![],
                wildcard: Some((9, 9)),
            })
        );
    }

    #[test]
    fn case7_wildcard_empty_with_param() {
        let mut recognizer = Recognizer::default();
        recognizer.add_path("/path/to/:id/*path").unwrap();

        let mut captures = None;
        assert_eq!(recognizer.recognize("/path/to/10/", &mut captures), Ok(0));
        assert_eq!(
            captures,
            Some(Captures {
                params: vec![(9, 11)],
                wildcard: Some((12, 12)),
            })
        );
    }

    #[test]
    fn case8_partially_matched() {
        let mut recognizer = Recognizer::default();
        recognizer.add_path("/path/to/foo").unwrap();
        recognizer.add_path("/path/to/bar").unwrap();

        // too short path
        assert_eq!(
            recognizer.recognize("/path/to/", &mut None),
            Err(RecognizeError::PartiallyMatched(&Candidates(indexset![
                0, 1
            ])))
        );

        // too long path
        assert_eq!(
            recognizer.recognize("/path/to/foo/baz", &mut None),
            Err(RecognizeError::PartiallyMatched(&Candidates(indexset![0])))
        );
    }

    #[test]
    fn case9_completely_mismatched() {
        let mut recognizer = Recognizer::default();
        recognizer.add_path("/path/to/foo").unwrap();

        // the suffix is different
        assert_eq!(
            recognizer.recognize("/path/to/baz", &mut None),
            Err(RecognizeError::NotMatched)
        );
    }

    #[test]
    fn case10_asterisk() {
        let mut recognizer = Recognizer::default();
        recognizer.add_path("*").unwrap();

        let mut captures = None;
        assert_eq!(recognizer.recognize("*", &mut captures), Ok(0));
        assert_eq!(captures, None);
    }

    #[test]
    fn case11_no_asterisk() {
        let mut recognizer = Recognizer::default();
        recognizer.add_path("/foo").unwrap();

        assert_eq!(
            recognizer.recognize("*", &mut None),
            Err(RecognizeError::NotMatched)
        );
    }
}

#[cfg(test)]
mod tests_tree {
    use {
        super::{Candidates, Node, NodeKind, Recognizer},
        indexmap::indexset,
    };

    macro_rules! t {
        ($test:ident, [$($path:expr),*], $expected:expr) => {
            #[test]
            fn $test() {
                let mut recognizer = Recognizer::default();
                for path in &[$($path),*] {
                    recognizer.add_path(path).unwrap();
                }
                assert_eq!(recognizer.tree.root, Some($expected));
            }
        };
        ($test:ident, [$($path:expr,)+], $expected:expr) => {
            t!($test, [$($path),*], $expected);
        };
    }

    #[test]
    fn case0() {
        let recognizer = Recognizer::default();
        assert_eq!(recognizer.tree.root, None);
    }

    t!(
        case1,
        ["/foo"],
        Node {
            kind: NodeKind::Static("/foo".into()),
            candidates: Candidates(indexset![0]),
            leaf: Some(0),
            children: vec![],
        }
    );

    t!(
        case2,
        ["/foo", "/bar"],
        Node {
            kind: NodeKind::Static("/".into()),
            candidates: Candidates(indexset![0, 1]),
            leaf: None,
            children: vec![
                Node {
                    kind: NodeKind::Static("foo".into()),
                    candidates: Candidates(indexset![0]),
                    leaf: Some(0),
                    children: vec![],
                },
                Node {
                    kind: NodeKind::Static("bar".into()),
                    candidates: Candidates(indexset![1]),
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
            candidates: Candidates(indexset![0, 1]),
            leaf: Some(0),
            children: vec![Node {
                kind: NodeKind::Static("bar".into()),
                candidates: Candidates(indexset![1]),
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
            candidates: Candidates(indexset![0]),
            children: vec![Node {
                kind: NodeKind::Param, // ":id"
                leaf: Some(0),
                candidates: Candidates(indexset![0]),
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
            candidates: Candidates(indexset![0, 1, 2, 3, 4]),
            children: vec![Node {
                kind: NodeKind::Static("/".into()),
                leaf: None,
                candidates: Candidates(indexset![1, 2, 3, 4]),
                children: vec![Node {
                    kind: NodeKind::Param, // ":name"
                    leaf: Some(2),
                    candidates: Candidates(indexset![1, 2, 3, 4]),
                    children: vec![Node {
                        kind: NodeKind::Static("/likes/".into()),
                        leaf: Some(1),
                        candidates: Candidates(indexset![1, 3, 4]),
                        children: vec![Node {
                            kind: NodeKind::Param, // ":id"
                            leaf: Some(4),
                            candidates: Candidates(indexset![3, 4]),
                            children: vec![Node {
                                kind: NodeKind::Static("/".into()),
                                leaf: Some(3),
                                candidates: Candidates(indexset![3]),
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
            candidates: Candidates(indexset![0]),
            children: vec![Node {
                kind: NodeKind::CatchAll, // "*path"
                leaf: Some(0),
                candidates: Candidates(indexset![0]),
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
            candidates: Candidates(indexset![0, 1]),
            children: vec![Node {
                kind: NodeKind::Static("/".into()),
                leaf: None,
                candidates: Candidates(indexset![1]),
                children: vec![Node {
                    kind: NodeKind::CatchAll, // "*path"
                    leaf: Some(1),
                    candidates: Candidates(indexset![1]),
                    children: vec![],
                }],
            }],
        }
    );

    #[test]
    fn failcase1() {
        let mut recognizer = Recognizer::default();
        assert!(recognizer.add_path("/foo").is_ok());
        assert!(recognizer.add_path("/:id").is_err());
    }

    #[test]
    fn failcase2() {
        let mut recognizer = Recognizer::default();
        assert!(recognizer.add_path("/foo/").is_ok());
        assert!(recognizer.add_path("/foo/*path").is_err());
    }

    #[test]
    fn failcase3() {
        let mut recognizer = Recognizer::default();
        assert!(recognizer.add_path("/:id").is_ok());
        assert!(recognizer.add_path("/foo").is_err());
    }

    #[test]
    fn failcase4() {
        let mut recognizer = Recognizer::default();
        assert!(recognizer.add_path("/foo/*path").is_ok());
        assert!(recognizer.add_path("/foo/").is_err());
    }

    #[test]
    fn failcase5() {
        let mut recognizer = Recognizer::default();
        assert!(recognizer.add_path("/:id").is_ok());
        assert!(recognizer.add_path("/:name").is_err());
    }

    #[test]
    fn failcase6() {
        let mut recognizer = Recognizer::default();
        assert!(recognizer.add_path("/:id").is_ok());
        assert!(recognizer.add_path("/*id").is_err());
    }

    #[test]
    fn failcase7() {
        let mut recognizer = Recognizer::default();
        assert!(recognizer.add_path("/*id").is_ok());
        assert!(recognizer.add_path("/:id").is_err());
    }

    #[test]
    fn failcase8() {
        let mut recognizer = Recognizer::default();
        assert!(recognizer.add_path("/path/to").is_ok());
        assert!(recognizer.add_path("/path/to").is_err());
    }
}
