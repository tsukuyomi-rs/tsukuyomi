#![cfg(test)]

use super::{Captures, Recognizer};

#[test]
fn case1_empty() {
    let mut recognizer = Recognizer::default();
    recognizer.add_route("/").unwrap();

    assert_eq!(recognizer.recognize("/"), Some((0, None,)));
}

#[test]
fn case2_multi_param() {
    let mut recognizer = Recognizer::default();
    recognizer.add_route("/files/:name/:id").unwrap();

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
    recognizer.add_route("/*path").unwrap();

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
    recognizer.add_route("/path/to/*path").unwrap();

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
    recognizer.add_route("/*path").unwrap();

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
    recognizer.add_route("/path/to/*path").unwrap();

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
    recognizer.add_route("/path/to/:id/*path").unwrap();

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
