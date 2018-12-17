//! A set of built-in `ModifyHandler`s.

use {
    crate::handler::{Handler, ModifyHandler},
    either::Either,
    http::Response,
};

/// Creates a `ModifyHandler` that overwrites the handling when receiving `OPTIONS`.
pub fn default_options<H>() -> impl ModifyHandler<
    H,
    Output = Either<Response<()>, H::Output>,
    Handler = self::default_options::DefaultOptions<H>, // private
>
where
    H: Handler,
{
    crate::handler::modify_handler(|inner: H| {
        let allowed_methods = inner.allowed_methods().cloned().map(|mut methods| {
            methods.extend(Some(http::Method::OPTIONS));
            methods
        });
        self::default_options::DefaultOptions {
            inner,
            allowed_methods,
        }
    })
}

mod default_options {
    use {
        crate::{
            future::{Poll, TryFuture},
            handler::{AllowedMethods, Handler},
            input::Input,
        },
        either::Either,
        http::{header::HeaderValue, Method, Response},
    };

    #[allow(missing_debug_implementations)]
    pub struct DefaultOptions<H> {
        pub(super) inner: H,
        pub(super) allowed_methods: Option<AllowedMethods>,
    }

    impl<H> Handler for DefaultOptions<H>
    where
        H: Handler,
    {
        type Output = Either<Response<()>, H::Output>;
        type Error = H::Error;
        type Handle = HandleDefaultOptions<H::Handle>;

        fn handle(&self) -> Self::Handle {
            HandleDefaultOptions {
                inner: self.inner.handle(),
                allowed_methods_value: self.allowed_methods().map(|m| m.to_header_value()),
            }
        }

        fn allowed_methods(&self) -> Option<&AllowedMethods> {
            self.allowed_methods.as_ref()
        }
    }

    #[allow(missing_debug_implementations)]
    pub struct HandleDefaultOptions<H> {
        inner: H,
        allowed_methods_value: Option<HeaderValue>,
    }

    impl<H> TryFuture for HandleDefaultOptions<H>
    where
        H: TryFuture,
    {
        type Ok = Either<Response<()>, H::Ok>;
        type Error = H::Error;

        #[inline]
        fn poll_ready(&mut self, input: &mut Input<'_>) -> Poll<Self::Ok, Self::Error> {
            if input.request.method() == Method::OPTIONS {
                let allowed_methods_value =
                    self.allowed_methods_value.take().unwrap_or_else(|| {
                        HeaderValue::from_static(
                            "GET, POST, PUT, DELETE, HEAD, OPTIONS, CONNECT, PATCH, TRACE",
                        )
                    });
                Ok(Either::Left(
                    Response::builder()
                        .status(http::StatusCode::NO_CONTENT)
                        .header(http::header::ALLOW, allowed_methods_value)
                        .body(())
                        .expect("should be a valid response"),
                )
                .into())
            } else {
                self.inner.poll_ready(input).map(|x| x.map(Either::Right))
            }
        }
    }
}

/// Creates a `ModifyHandler` that converts the output value using the specified function.
pub fn map_output<H, F, T>(
    f: F,
) -> impl ModifyHandler<H, Output = T, Handler = self::map_output::MapOutput<H, F>>
where
    H: Handler,
    F: Fn(H::Output) -> T + Clone,
{
    crate::handler::modify_handler(move |handler: H| self::map_output::MapOutput {
        handler,
        f: f.clone(),
    })
}

mod map_output {
    use crate::{
        future::{Poll, TryFuture},
        handler::{AllowedMethods, Handler},
        input::Input,
    };

    #[allow(missing_debug_implementations)]
    pub struct MapOutput<H, F> {
        pub(super) handler: H,
        pub(super) f: F,
    }

    impl<H, F, T> Handler for MapOutput<H, F>
    where
        H: Handler,
        F: Fn(H::Output) -> T + Clone,
    {
        type Output = T;
        type Error = H::Error;
        type Handle = HandleMapOutput<H::Handle, F>;

        fn handle(&self) -> Self::Handle {
            HandleMapOutput {
                handle: self.handler.handle(),
                f: self.f.clone(),
            }
        }

        fn allowed_methods(&self) -> Option<&AllowedMethods> {
            self.handler.allowed_methods()
        }
    }

    #[allow(missing_debug_implementations)]
    pub struct HandleMapOutput<H, F> {
        handle: H,
        f: F,
    }

    impl<H, F, T> TryFuture for HandleMapOutput<H, F>
    where
        H: TryFuture,
        F: Fn(H::Ok) -> T,
    {
        type Ok = T;
        type Error = H::Error;

        #[inline]
        fn poll_ready(&mut self, input: &mut Input<'_>) -> Poll<Self::Ok, Self::Error> {
            self.handle
                .poll_ready(input)
                .map(|x| x.map(|out| (self.f)(out)))
        }
    }
}
