extern crate tsukuyomi;
extern crate tsukuyomi_fs;

use tsukuyomi::app::{scope, App};
use tsukuyomi_fs::Staticfiles;

#[test]
#[ignore]
fn compiletest_staticfiles() {
    drop(
        App::builder()
            .mount(scope::builder().with(Staticfiles::new("./public"))) //
            .finish()
            .unwrap(),
    );
}
