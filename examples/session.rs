extern crate tsukuyomi;

#[cfg(not(feature = "session"))]
fn main() {
    println!("This example works only if the feature 'session' is enabled");
}

#[cfg(feature = "session")]
fn main() -> tsukuyomi::AppResult<()> {
    use tsukuyomi::future::{ready, Ready};
    use tsukuyomi::session::ContextSessionExt;
    use tsukuyomi::{App, Error};

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
