extern crate tsukuyomi;

use tsukuyomi::session::InputSessionExt;
use tsukuyomi::{App, Input};

fn handler(input: &mut Input) -> tsukuyomi::Result<String> {
    if let Some(foo) = input.session().get::<String>("foo")? {
        Ok(format!("foo = {}\n", foo))
    } else {
        input.session().set::<String>("foo", "bar".into())?;
        Ok("set: foo = bar\n".into())
    }
}

fn main() -> tsukuyomi::AppResult<()> {
    let app = App::builder()
        .mount("/", |r| {
            r.get("/").handle(handler);
        })
        .finish()?;

    tsukuyomi::run(app)
}
