extern crate askama;
extern crate tsukuyomi;
extern crate tsukuyomi_askama;

#[test]
fn test_template() {
    use askama::Template;
    use tsukuyomi::app::App;
    use tsukuyomi::route;

    #[derive(Template, tsukuyomi_askama::Responder)]
    #[template(path = "index.html")]
    struct Index {
        name: &'static str,
    }

    let app = App::builder()
        .route(route::index().reply(|| Index { name: "Alice" }))
        .finish()
        .unwrap();
    drop(app);
}
