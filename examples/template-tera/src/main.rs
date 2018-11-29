extern crate http;
extern crate mime_guess;
extern crate serde;
extern crate tera;
extern crate tsukuyomi;

use tsukuyomi::output::Responder;

#[derive(Debug, serde::Serialize, Responder)]
#[responder(respond_to = "crate::support_tera::respond_to")]
struct Index {
    name: String,
}

impl crate::support_tera::TemplateExt for Index {
    fn template_name(&self) -> &str {
        "index.html"
    }
}

fn main() -> tsukuyomi::server::Result<()> {
    let engine = tera::compile_templates!(concat!(env!("CARGO_MANIFEST_DIR"), "/templates/**/*"));

    tsukuyomi::App::builder()
        .with(tsukuyomi::app::scope::state(engine))
        .with(
            tsukuyomi::app::route!("/:name") //
                .reply(|name| Index { name }),
        ) //
        .build_server()?
        .run()
}

mod support_tera {
    use {
        http::{header::HeaderValue, Response},
        tera::Tera,
    };

    pub trait TemplateExt {
        fn template_name(&self) -> &str;
        fn extension(&self) -> Option<&str> {
            None
        }
    }

    #[cfg_attr(feature = "cargo-clippy", allow(needless_pass_by_value))]
    pub fn respond_to<T>(
        ctx: T,
        input: &mut tsukuyomi::input::Input<'_>,
    ) -> tsukuyomi::error::Result<Response<String>>
    where
        T: serde::Serialize + TemplateExt,
    {
        let engine = input.states.try_get::<Tera>().ok_or_else(|| {
            tsukuyomi::error::internal_server_error(
                "Tera template engine is not available in this scope",
            )
        })?;

        let mut response = engine
            .render(ctx.template_name(), &ctx)
            .map(Response::new)
            .map_err(tsukuyomi::error::internal_server_error)?;

        let content_type = HeaderValue::from_static(
            ctx.extension()
                .and_then(mime_guess::get_mime_type_str)
                .unwrap_or("text/html; charset=utf-8"),
        );
        response
            .headers_mut()
            .insert(http::header::CONTENT_TYPE, content_type);

        Ok(response)
    }
}
