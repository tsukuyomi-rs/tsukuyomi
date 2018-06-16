extern crate tsukuyomi;

use tsukuyomi::future::{ready, Ready};
use tsukuyomi::session::InputSessionExt;
use tsukuyomi::{App, Error};

fn main() -> tsukuyomi::AppResult<()> {
    let app = App::builder()
        .mount("/", |r| {
            r.get("/", |cx| -> Ready<_> {
                ready((|| -> Result<String, Error> {
                    if let Some(foo) = cx.session().get::<String>("foo")? {
                        Ok(format!("foo = {}\n", foo))
                    } else {
                        cx.session().set::<String>("foo", "bar".into())?;
                        Ok("set: foo = bar\n".into())
                    }
                })())
            });
        })
        .finish()?;

    tsukuyomi::run(app)
}
