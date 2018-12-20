mod integration_tests;

#[test]
fn test_version_sync() {
    version_sync::assert_html_root_url_updated!("src/lib.rs");
}

#[test]
#[should_panic]
fn test_catch_unwind() {
    fn inner() -> tsukuyomi_server::Result<()> {
        use tsukuyomi::{config::prelude::*, App};

        let app = App::create(
            path!("/") //
                .to(endpoint::call(|| -> &'static str {
                    panic!("explicit panic")
                })),
        )?;

        let mut server = tsukuyomi_server::test::server(app)?;
        server.perform("/")?;

        Ok(())
    }

    if let Err(err) = inner() {
        eprintln!("unexpected error: {:?}", err);
    }
}

#[test]
fn test_current_thread() -> tsukuyomi_server::Result<()> {
    use tsukuyomi::{app::LocalApp, config::prelude::*};

    let ptr = std::rc::Rc::new(());

    let app = LocalApp::create(
        path!("/") //
            .to(endpoint::call(move || {
                let _ptr = ptr.clone();
                "dummy"
            })),
    )?;

    let mut server = tsukuyomi_server::test::current_thread_server(app)?;
    let _ = server.perform("/")?;

    Ok(())
}
