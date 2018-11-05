#![allow(missing_docs)]

pub use crate::app::route::Builder;

macro_rules! define_route {
    ($($method:ident => $METHOD:ident,)*) => {$(
        pub fn $method(uri: impl AsRef<str>) -> Builder<()> {
            Builder::new(()).method(http::Method::$METHOD).uri(uri)
        }

        #[macro_export(local_inner_macros)]
        macro_rules! $method {
            ($uri:expr) => {{
                enum __Dummy {}
                impl __Dummy {
                    route_impl!($method $uri);
                }
                __Dummy::route()
            }};
        }
    )*}
}

define_route! {
    get => GET,
    post => POST,
    put => PUT,
    delete => DELETE,
    head => HEAD,
    options => OPTIONS,
    connect => CONNECT,
    patch => PATCH,
    trace => TRACE,
}

// Equivalent to `get("/")`
pub fn index() -> Builder<()> {
    self::get("/")
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::extractor::Extractor;

    fn generated() -> Builder<impl Extractor<Output = (u32, String)>> {
        super::get("/:id/:name")
            .with(crate::extractor::param::pos(0))
            .with(crate::extractor::param::pos(1))
    }

    #[test]
    #[ignore]
    fn compiletest1() {
        drop(
            crate::app(|scope| {
                scope.route(generated().reply(|id: u32, name: String| {
                    drop((id, name));
                    "dummy"
                }));
            }).expect("failed to construct App"),
        );
    }

    #[test]
    #[ignore]
    fn compiletest2() {
        drop(
            crate::app(|scope| {
                scope.route(generated().with(crate::extractor::body::plain()).reply(
                    |id: u32, name: String, body: String| {
                        drop((id, name, body));
                        "dummy"
                    },
                ));
            }).expect("failed to construct App"),
        );
    }
}
