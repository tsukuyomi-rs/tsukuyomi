//! The implementation of route recognizer based on radix tree.

// The original implementation is located at https://github.com/ubnt-intrepid/susanoo

use failure::Error;
use std::{cmp, mem, str};

/// Calculate the endpoint of longest common prefix between the two slices.
fn lcp(s1: &str, s2: &str) -> usize {
    s1.bytes()
        .zip(s2.bytes())
        .position(|(s1, s2)| s1 != s2)
        .unwrap_or_else(|| cmp::min(s1.len(), s2.len()))
}

fn find_wildcard_begin(path: &str, offset: usize) -> usize {
    path.bytes()
        .skip(offset)
        .position(|b| b == b':' || b == b'*')
        .map(|i| i + offset)
        .unwrap_or_else(|| path.len())
}

fn find_wildcard_end(path: &str, offset: usize) -> Result<usize, Error> {
    let path = path.as_bytes();
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
    path: String,
    leaf: Option<usize>,
    children: Vec<Node>,
}

impl Node {
    fn new<S: Into<String>>(path: S) -> Node {
        Node {
            path: path.into(),
            leaf: None,
            children: vec![],
        }
    }

    fn add_child<S: Into<String>>(&mut self, path: S) -> Result<&mut Node, Error> {
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
        match self.path.bytes().next() {
            Some(b':') | Some(b'*') => true,
            _ => false,
        }
    }

    fn add_path(&mut self, path: &str, value: usize) -> Result<(), Error> {
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
            let c = path.bytes().nth(offset);
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
                    if &path[offset..end] != &n.path[..] {
                        bail!("wildcard conflict");
                    }
                    if end == path.len() {
                        break 'walk;
                    }
                    offset = end;
                }
                Some(c) => {
                    if n.children.iter().any(|ch| ch.is_wildcard()) {
                        bail!("A wildcard node has already inserted.");
                    }

                    // Check if a child with the next path byte exists
                    for pos in 0..n.children.len() {
                        if n.children[pos].path.as_bytes()[0] == c {
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

        if n.children.iter().any(|ch| ch.path.starts_with('*')) {
            bail!("catch-all conflict");
        }

        n.leaf = Some(value);
        Ok(())
    }

    fn insert_child(&mut self, path: &str, value: usize) -> Result<(), Error> {
        let mut pos = 0;
        let mut n = self;

        'walk: while pos < path.len() {
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

        n.leaf = Some(value);
        return Ok(());
    }

    fn get_value<'r, 'p>(&'r self, path: &'p str) -> Option<(usize, Vec<(usize, usize)>)> {
        let mut offset = 0;
        let mut n = self;
        let mut captures = Vec::new();

        'walk: loop {
            if path.len() > offset + n.path.len() {
                if &path[offset..offset + n.path.len()] == &n.path[..] {
                    offset += n.path.len();

                    'inner: loop {
                        let c = path.as_bytes()[offset];
                        for ch in &n.children {
                            if ch.path.starts_with(':') || ch.path.starts_with('*') {
                                break 'inner;
                            }
                            if ch.path.as_bytes()[0] == c {
                                n = ch;
                                continue 'walk;
                            }
                        }
                        return None;
                    }

                    n = &n.children[0];
                    match n.path.as_bytes()[0] {
                        b':' => {
                            let span = path[offset..]
                                .bytes()
                                .position(|b| b == b'/')
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
            } else if &path[offset..] == &n.path[..] {
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
pub struct Builder<T> {
    root: Option<Node>,
    values: Vec<T>,
    result: Result<(), Error>,
}

impl<T> Builder<T> {
    /// Add a path to this builder with a value of `T`.
    pub fn insert(&mut self, path: &str, value: T) -> &mut Self {
        if self.result.is_ok() {
            self.result = self.insert_inner(path, value);
        }
        self
    }

    fn insert_inner(&mut self, path: &str, value: T) -> Result<(), Error> {
        if !path.is_ascii() {
            bail!("The path must be a sequence of ASCII characters");
        }

        let index = self.values.len();

        if let Some(ref mut root) = self.root {
            root.add_path(path, index)?;
            self.values.push(value);
            return Ok(());
        }

        let pos = find_wildcard_begin(path, 0);
        self.root
            .get_or_insert(Node::new(&path[..pos]))
            .insert_child(&path[pos..], index)?;
        self.values.push(value);

        Ok(())
    }

    /// Finalize the build process and create an instance of `Recognizer`.
    pub fn finish(&mut self) -> Result<Recognizer<T>, Error> {
        let Builder { root, values, result } = mem::replace(self, Recognizer::builder());
        result?;
        Ok(Recognizer { root, values })
    }
}

/// A route recognizer based on radix tree.
#[derive(Debug)]
pub struct Recognizer<T> {
    root: Option<Node>,
    values: Vec<T>,
}

impl<T> Recognizer<T> {
    /// Creates an instance of builder object for constructing a value of this type.
    ///
    /// See the documentations of `Builder` for details.
    pub fn builder() -> Builder<T> {
        Builder {
            root: None,
            values: vec![],
            result: Ok(()),
        }
    }

    /// Traverses the given path and returns a reference to registered value of "T" if matched.
    ///
    /// At the same time, this method returns a sequence of pairs which indicates the range of
    /// substrings extracted as parameters.
    pub fn recognize(&self, path: &str) -> Option<(&T, Vec<(usize, usize)>)> {
        let (index, captures) = self.root.as_ref()?.get_value(path)?;
        let values = &self.values[index];
        Some((values, captures))
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
                    let mut builder = Recognizer::<usize>::builder();
                    for (i, path) in [$($path),*].into_iter().enumerate() {
                        builder.insert(path, i);
                    }
                    let recognizer = builder.finish().unwrap();
                    assert_eq!(recognizer.root, Some($expected));
                }
            };
            ($test:ident, [$($path:expr,)+], $expected:expr) => {
                t!($test, [$($path),*], $expected);
            };
        }

        #[test]
        fn case0() {
            let tree = Recognizer::<()>::builder().finish().unwrap();
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
            assert!(
                Recognizer::<()>::builder()
                    .insert("/foo", ())
                    .insert("/:id", ())
                    .finish()
                    .is_err()
            );
        }

        #[test]
        fn failcase2() {
            assert!(
                Recognizer::<()>::builder()
                    .insert("/foo/", ())
                    .insert("/foo/*path", ())
                    .finish()
                    .is_err()
            );
        }

        #[test]
        fn failcase3() {
            assert!(
                Recognizer::<()>::builder()
                    .insert("/:id", ())
                    .insert("/foo", ())
                    .finish()
                    .is_err()
            );
        }

        #[test]
        fn failcase4() {
            assert!(
                Recognizer::<()>::builder()
                    .insert("/foo/*path", ())
                    .insert("/foo/", ())
                    .finish()
                    .is_err()
            );
        }

        #[test]
        fn failcase5() {
            assert!(
                Recognizer::<()>::builder()
                    .insert("/:id", ())
                    .insert("/:name", ())
                    .finish()
                    .is_err()
            );
        }
    }

    mod get {
        use super::super::Recognizer;

        #[test]
        fn case1() {
            let recognizer = Recognizer::<()>::builder().insert("/", ()).finish().unwrap();
            assert_eq!(recognizer.recognize("/"), Some((&(), vec![])));
        }

        #[test]
        fn case2() {
            let recognizer = Recognizer::<usize>::builder()
                .insert("/files/:name/:id", 42usize)
                .finish()
                .unwrap();
            assert_eq!(
                recognizer.recognize("/files/readme/0"),
                Some((&42, vec![(7, 13), (14, 15)]))
            );
        }

        #[test]
        fn case3() {
            let recognizer = Recognizer::<usize>::builder().insert("/*path", 42).finish().unwrap();
            assert_eq!(recognizer.recognize("/path/to/readme.txt"), Some((&42, vec![(1, 19)])));
        }
    }
}
