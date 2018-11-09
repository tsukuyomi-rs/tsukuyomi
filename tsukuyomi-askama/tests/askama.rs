extern crate askama;
extern crate tsukuyomi;
extern crate tsukuyomi_askama;

use askama::Template;
use tsukuyomi::app::App;
use tsukuyomi::output::Responder;
use tsukuyomi::route;

#[inline]
fn assert_impl<T: Responder>(x: T) -> T {
    x
}

#[test]
fn test_template() {
    #[derive(Template, Responder)]
    #[template(path = "index.html")]
    #[responder(respond_to = "tsukuyomi_askama::respond_to")]
    struct Index {
        name: &'static str,
    }
    drop(
        App::build(|scope| {
            scope.route(route::index().reply(|| assert_impl(Index { name: "Alice" })));
        }).unwrap(),
    );
}
