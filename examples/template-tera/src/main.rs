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

fn main() {
    let engine = tera::compile_templates!(concat!(env!("CARGO_MANIFEST_DIR"), "/templates/**/*"));

    let app = tsukuyomi::app(|scope| {
        scope.state(engine);
        scope.route(tsukuyomi::route!("/:name").reply(|name| Index { name }));
    }).unwrap();

    tsukuyomi::server(app).run_forever().unwrap()
}

mod support_tera {
    use http::header::HeaderValue;
    use http::Response;
    use tera::Tera;
    use tsukuyomi::input::Input;

    pub trait TemplateExt {
        fn template_name(&self) -> &str;
        fn extension(&self) -> Option<&str> {
            None
        }
    }

    pub fn respond_to<T>(
        ctx: T,
        input: &mut Input<'_>,
    ) -> tsukuyomi::error::Result<Response<String>>
    where
        T: serde::Serialize + TemplateExt,
    {
        let engine = input.state::<Tera>().ok_or_else(|| {
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
