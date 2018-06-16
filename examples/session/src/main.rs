extern crate tsukuyomi;

use tsukuyomi::future::{ready, Ready};
use tsukuyomi::session::InputSessionExt;
use tsukuyomi::{App, Error, Input};

fn main() -> tsukuyomi::AppResult<()> {
    let app = App::builder()
        .mount("/", |r| {
            r.get("/", || -> Ready<_> {
                ready(Input::with(|input| -> Result<String, Error> {
                    if let Some(foo) = input.session().get::<String>("foo")? {
                        Ok(format!("foo = {}\n", foo))
                    } else {
                        input.session().set::<String>("foo", "bar".into())?;
                        Ok("set: foo = bar\n".into())
                    }
                }))
            });
        })
        .finish()?;

    tsukuyomi::run(app)
}
