extern crate cargo_version_sync;

#[test]
fn test_version_sync() {
    cargo_version_sync::assert_version_sync();
}
