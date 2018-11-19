extern crate tsukuyomi;
extern crate tsukuyomi_fs;

use tsukuyomi_fs::Staticfiles;

#[test]
#[ignore]
fn compiletest_staticfiles() {
    drop(
        tsukuyomi::app!()
            .with(Staticfiles::new("./public")) //
            .build()
            .unwrap(),
    );
}
