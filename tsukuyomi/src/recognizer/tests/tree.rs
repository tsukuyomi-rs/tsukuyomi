use super::{Node, PathKind, Tree};

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
        path: PathKind::segment("/foo"),
        leaf: Some(0),
        children: vec![],
    }
);

t!(
    case2,
    ["/foo", "/bar"],
    Node {
        path: PathKind::segment("/"),
        leaf: None,
        children: vec![
            Node {
                path: PathKind::segment("foo"),
                leaf: Some(0),
                children: vec![],
            },
            Node {
                path: PathKind::segment("bar"),
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
        path: PathKind::segment("/foo"),
        leaf: Some(0),
        children: vec![Node {
            path: PathKind::segment("bar"),
            leaf: Some(1),
            children: vec![],
        }],
    }
);

t!(
    param_case1,
    ["/:id"],
    Node {
        path: PathKind::segment("/"),
        leaf: None,
        children: vec![Node {
            path: PathKind::Param, // ":id"
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
        path: PathKind::segment("/files"),
        leaf: Some(0),
        children: vec![Node {
            path: PathKind::segment("/"),
            leaf: None,
            children: vec![Node {
                path: PathKind::Param, // ":name"
                leaf: Some(2),
                children: vec![Node {
                    path: PathKind::segment("/likes/"),
                    leaf: Some(1),
                    children: vec![Node {
                        path: PathKind::Param, // ":id"
                        leaf: Some(4),
                        children: vec![Node {
                            path: PathKind::segment("/"),
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
        path: PathKind::segment("/"),
        leaf: None,
        children: vec![Node {
            path: PathKind::CatchAll, // "*path"
            leaf: Some(0),
            children: vec![],
        }],
    }
);

t!(
    catch_all_case2,
    ["/files", "/files/*path"],
    Node {
        path: PathKind::segment("/files"),
        leaf: Some(0),
        children: vec![Node {
            path: PathKind::segment("/"),
            leaf: None,
            children: vec![Node {
                path: PathKind::CatchAll, // "*path"
                leaf: Some(1),
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
