#![allow(missing_docs)]

//! The implementation of route recognizer.

// NOTE: The original implementation was imported from https://github.com/ubnt-intrepid/susanoo

use failure::Error;
use std::{cmp, mem, str};

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

#[derive(Debug, PartialEq)]
struct Node {
    path: Vec<u8>,
    leaf: Option<usize>,
    children: Vec<Node>,
}

impl Node {
    fn new<S: Into<Vec<u8>>>(path: S) -> Node {
        Node {
            path: path.into(),
            leaf: None,
            children: vec![],
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
        match self.path.iter().next() {
            Some(b':') | Some(b'*') => true,
            _ => false,
        }
    }

    fn add_path(&mut self, path: &[u8], value: usize) -> Result<(), Error> {
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

    fn insert_child(&mut self, path: &[u8], value: usize) -> Result<(), Error> {
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

    fn get_value<'r, 'p>(&'r self, path: &'p [u8]) -> Option<(usize, Vec<(usize, usize)>)> {
        let mut offset = 0;
        let mut n = self;
        let mut captures = Vec::new();

        'walk: loop {
            if path.len() > offset + n.path.len() {
                if path[offset..offset + n.path.len()] == n.path[..] {
                    offset += n.path.len();

                    enum Kind {
                        Param,
                        Segment(usize),
                    }

                    let ch_kind = n.children
                        .iter()
                        .enumerate()
                        .filter_map({
                            let pred = path[offset];
                            move |(pos, ch)| match ch.path.get(0)? {
                                b':' | b'*' => Some(Kind::Param),
                                &c if c == pred => Some(Kind::Segment(pos)),
                                _ => None,
                            }
                        })
                        .next()?;

                    match ch_kind {
                        Kind::Param => {
                            n = &n.children[0];
                            match n.path[0] {
                                b':' => {
                                    let span = path[offset..]
                                        .into_iter()
                                        .position(|&b| b == b'/')
                                        .unwrap_or(path.len() - offset);
                                    captures.push((offset, offset + span));
                                    if span < path.len() - offset {
                                        if !n.children.is_empty() {
                                            offset += span;
                                            n = &n.children[0];
                                            continue 'walk;
                                        }
                                        return None;
                                    }
                                    break 'walk;
                                }
                                b'*' => {
                                    captures.push((offset, path.len()));
                                    break 'walk;
                                }
                                _ => unreachable!("invalid node type"),
                            }
                        }
                        Kind::Segment(pos) => {
                            n = &n.children[pos];
                            continue 'walk;
                        }
                    }
                }
            } else if path[offset..] == n.path[..] {
                break 'walk;
            }

            return None;
        }

        let index = n.leaf?;
        Some((index, captures))
    }
}

/// A builder object for constructing the instance of `Recognizer`.
#[derive(Debug)]
pub(crate) struct Builder {
    root: Option<Node>,
    paths: Vec<String>,
}

impl Builder {
    /// Add a path to this builder with a value of `T`.
    pub(crate) fn push<T>(&mut self, path: T) -> Result<(), Error>
    where
        T: Into<String>,
    {
        let path = path.into();
        if !path.is_ascii() {
            bail!("The path must be a sequence of ASCII characters");
        }

        let index = self.paths.len();

        if let Some(ref mut root) = self.root {
            root.add_path(path.as_bytes(), index)?;
            self.paths.push(path);
            return Ok(());
        }

        let pos = find_wildcard_begin(path.as_bytes(), 0);
        self.root
            .get_or_insert(Node::new(&path[..pos]))
            .insert_child(path[pos..].as_bytes(), index)?;
        self.paths.push(path);

        Ok(())
    }

    /// Finalize the build process and create an instance of `Recognizer`.
    pub(crate) fn finish(&mut self) -> Recognizer {
        let Builder { root, paths } = mem::replace(self, Recognizer::builder());
        Recognizer { root, paths }
    }
}

/// A route recognizer based on radix tree.
#[derive(Debug)]
pub(crate) struct Recognizer {
    root: Option<Node>,
    paths: Vec<String>,
}

impl Recognizer {
    /// Creates an instance of builder object for constructing a value of this type.
    ///
    /// See the documentations of `Builder` for details.
    pub(crate) fn builder() -> Builder {
        Builder {
            root: None,
            paths: vec![],
        }
    }

    /// Traverses the given path and returns a reference to registered value of "T" if matched.
    ///
    /// At the same time, this method returns a sequence of pairs which indicates the range of
    /// substrings extracted as parameters.
    pub(crate) fn recognize(&self, path: &str) -> Option<(usize, Vec<(usize, usize)>)> {
        self.root.as_ref()?.get_value(path.as_bytes())
    }
}

#[cfg(test)]
mod tests {
    mod insert {
        use super::super::{Node, Recognizer};

        macro_rules! t {
            ($test:ident, [$($path:expr),*], $expected:expr) => {
                #[test]
                fn $test() {
                    let mut builder = Recognizer::builder();
                    for &path in &[$($path),*] {
                        builder.push(path).unwrap();
                    }
                    let recognizer = builder.finish();
                    assert_eq!(recognizer.root, Some($expected));
                }
            };
            ($test:ident, [$($path:expr,)+], $expected:expr) => {
                t!($test, [$($path),*], $expected);
            };
        }

        #[test]
        fn case0() {
            let tree = Recognizer::builder().finish();
            assert_eq!(tree.root, None);
        }

        t!(
            case1,
            ["/foo"],
            Node {
                path: "/foo".into(),
                leaf: Some(0),
                children: vec![],
            }
        );

        t!(
            case2,
            ["/foo", "/bar"],
            Node {
                path: "/".into(),
                leaf: None,
                children: vec![
                    Node {
                        path: "foo".into(),
                        leaf: Some(0),
                        children: vec![],
                    },
                    Node {
                        path: "bar".into(),
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
                path: "/foo".into(),
                leaf: Some(0),
                children: vec![Node {
                    path: "bar".into(),
                    leaf: Some(1),
                    children: vec![],
                }],
            }
        );

        t!(
            param_case1,
            ["/:id"],
            Node {
                path: "/".into(),
                leaf: None,
                children: vec![Node {
                    path: ":id".into(),
                    leaf: Some(0),
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
                path: "/files".into(),
                leaf: Some(0),
                children: vec![Node {
                    path: "/".into(),
                    leaf: None,
                    children: vec![Node {
                        path: ":name".into(),
                        leaf: Some(2),
                        children: vec![Node {
                            path: "/likes/".into(),
                            leaf: Some(1),
                            children: vec![Node {
                                path: ":id".into(),
                                leaf: Some(4),
                                children: vec![Node {
                                    path: "/".into(),
                                    leaf: Some(3),
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
                path: "/".into(),
                leaf: None,
                children: vec![Node {
                    path: "*path".into(),
                    leaf: Some(0),
                    children: vec![],
                }],
            }
        );

        t!(
            catch_all_case2,
            ["/files", "/files/*path"],
            Node {
                path: "/files".into(),
                leaf: Some(0),
                children: vec![Node {
                    path: "/".into(),
                    leaf: None,
                    children: vec![Node {
                        path: "*path".into(),
                        leaf: Some(1),
                        children: vec![],
                    }],
                }],
            }
        );

        #[test]
        fn failcase1() {
            let mut builder = Recognizer::builder();
            assert!(builder.push("/foo").is_ok());
            assert!(builder.push("/:id").is_err());
        }

        #[test]
        fn failcase2() {
            let mut builder = Recognizer::builder();
            assert!(builder.push("/foo/").is_ok());
            assert!(builder.push("/foo/*path").is_err());
        }

        #[test]
        fn failcase3() {
            let mut builder = Recognizer::builder();
            assert!(builder.push("/:id").is_ok());
            assert!(builder.push("/foo").is_err());
        }

        #[test]
        fn failcase4() {
            let mut builder = Recognizer::builder();
            assert!(builder.push("/foo/*path").is_ok());
            assert!(builder.push("/foo/").is_err());
        }

        #[test]
        fn failcase5() {
            let mut builder = Recognizer::builder();
            assert!(builder.push("/:id").is_ok());
            assert!(builder.push("/:name").is_err());
        }

        #[test]
        fn failcase6() {
            let mut builder = Recognizer::builder();
            assert!(builder.push("/:id").is_ok());
            assert!(builder.push("/*id").is_err());
        }

        #[test]
        fn failcase7() {
            let mut builder = Recognizer::builder();
            assert!(builder.push("/*id").is_ok());
            assert!(builder.push("/:id").is_err());
        }

        #[test]
        fn failcase8() {
            let mut builder = Recognizer::builder();
            assert!(builder.push("/path/to").is_ok());
            assert!(builder.push("/path/to").is_err());
        }
    }

    mod recognize {
        use super::super::Recognizer;

        #[test]
        fn case1_empty() {
            let mut builder = Recognizer::builder();
            builder.push("/").unwrap();
            let recognizer = builder.finish();
            assert_eq!(recognizer.recognize("/"), Some((0, vec![])));
        }

        #[test]
        fn case2_multi_param() {
            let mut builder = Recognizer::builder();
            builder.push("/files/:name/:id").unwrap();
            let recognizer = builder.finish();

            assert_eq!(
                recognizer.recognize("/files/readme/0"),
                Some((0, vec![(7, 13), (14, 15)]))
            );
        }

        #[test]
        fn case3_wildcard_root() {
            let mut builder = Recognizer::builder();
            builder.push("/*path").unwrap();
            let recognizer = builder.finish();
            assert_eq!(recognizer.recognize("/path/to/readme.txt"), Some((0, vec![(1, 19)])));
        }

        #[test]
        fn case4_wildcard_subdir() {
            let mut builder = Recognizer::builder();
            builder.push("/path/to/*path").unwrap();
            let recognizer = builder.finish();
            assert_eq!(recognizer.recognize("/path/to/readme.txt"), Some((0, vec![(9, 19)])));
        }

        // The following test cases are for catching the unexpected behaviors.

        #[test]
        fn case5_wildcard_with_empty_root() {
            let mut builder = Recognizer::builder();
            builder.push("/*path").unwrap();
            let recognizer = builder.finish();
            assert_eq!(recognizer.recognize("/"), None);
            //assert_eq!(recognizer.recognize("/"), Some((0, vec![(1, 1)])));
        }

        #[test]
        fn case6_wildcard_with_empty_subdir() {
            let mut builder = Recognizer::builder();
            builder.push("/path/to/*path").unwrap();
            let recognizer = builder.finish();
            assert_eq!(recognizer.recognize("/path/to/"), None);
            //assert_eq!(recognizer.recognize("/path/to/"), Some((0, vec![(9, 9)])));
        }
    }
}
