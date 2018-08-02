#![cfg(test)]

use super::super::Recognizer;
use super::Node;

macro_rules! t {
    ($test:ident, [$($path:expr),*], $expected:expr) => {
        #[test]
        fn $test() {
            let mut recognizer = Recognizer::default();
            for &path in &[$($path),*] {
                recognizer.add_route(path).unwrap();
            }
            assert_eq!(recognizer.root, Some($expected));
        }
    };
    ($test:ident, [$($path:expr,)+], $expected:expr) => {
        t!($test, [$($path),*], $expected);
    };
}

#[test]
fn case0() {
    let tree = Recognizer::default();
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
    let mut recognizer = Recognizer::default();
    assert!(recognizer.add_route("/foo").is_ok());
    assert!(recognizer.add_route("/:id").is_err());
}

#[test]
fn failcase2() {
    let mut recognizer = Recognizer::default();
    assert!(recognizer.add_route("/foo/").is_ok());
    assert!(recognizer.add_route("/foo/*path").is_err());
}

#[test]
fn failcase3() {
    let mut recognizer = Recognizer::default();
    assert!(recognizer.add_route("/:id").is_ok());
    assert!(recognizer.add_route("/foo").is_err());
}

#[test]
fn failcase4() {
    let mut recognizer = Recognizer::default();
    assert!(recognizer.add_route("/foo/*path").is_ok());
    assert!(recognizer.add_route("/foo/").is_err());
}

#[test]
fn failcase5() {
    let mut recognizer = Recognizer::default();
    assert!(recognizer.add_route("/:id").is_ok());
    assert!(recognizer.add_route("/:name").is_err());
}

#[test]
fn failcase6() {
    let mut recognizer = Recognizer::default();
    assert!(recognizer.add_route("/:id").is_ok());
    assert!(recognizer.add_route("/*id").is_err());
}

#[test]
fn failcase7() {
    let mut recognizer = Recognizer::default();
    assert!(recognizer.add_route("/*id").is_ok());
    assert!(recognizer.add_route("/:id").is_err());
}

#[test]
fn failcase8() {
    let mut recognizer = Recognizer::default();
    assert!(recognizer.add_route("/path/to").is_ok());
    assert!(recognizer.add_route("/path/to").is_err());
}
