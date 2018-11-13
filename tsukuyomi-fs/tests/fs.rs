extern crate tsukuyomi;
extern crate tsukuyomi_fs;

use tsukuyomi::app::{scope, App};
use tsukuyomi_fs::Staticfiles;

#[test]
#[ignore]
fn compiletest_staticfiles() {
    drop(
        App::builder()
            .mount(
                scope::builder().with(
                    Staticfiles::new("./public")
                        .follow_links(true)
                        .same_file_system(false)
                        .filter_entry(|entry| {
                            entry
                                .file_name()
                                .to_str()
                                .map(|s| s.starts_with('.'))
                                .unwrap_or(false)
                        }),
                ),
            ) //
            .finish()
            .unwrap(),
    );
}
