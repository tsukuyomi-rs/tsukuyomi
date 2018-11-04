extern crate askama;
extern crate tsukuyomi;
extern crate tsukuyomi_askama;

use askama::Template;
use tsukuyomi::app::App;
use tsukuyomi::route;
use tsukuyomi_askama::TemplateResponder;

#[inline]
fn assert_impl<T: TemplateResponder>(x: T) -> T {
    x
}

#[test]
fn test_template() {
    #[derive(Template, TemplateResponder)]
    #[template(path = "index.html")]
    struct Index {
        name: &'static str,
    }

    let app = App::builder()
        .route(route::index().reply(|| assert_impl(Index { name: "Alice" })))
        .finish()
        .unwrap();
    drop(app);
}
