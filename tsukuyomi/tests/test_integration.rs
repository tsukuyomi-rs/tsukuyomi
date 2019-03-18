mod cases;

#[test]
fn test_version_sync() {
    version_sync::assert_html_root_url_updated!("src/lib.rs");
}
