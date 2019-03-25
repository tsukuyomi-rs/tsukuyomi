//! A set of built-in `ModifyHandler`s.

pub use self::{default_options::DefaultOptions, map_output::MapOutput};

/// Creates a `ModifyHandler` that overwrites the handling when receiving `OPTIONS`.
pub fn default_options() -> DefaultOptions {
    DefaultOptions(())
}

mod default_options {
    use {
        crate::{
            future::{Poll, TryFuture},
            handler::{metadata::Metadata, Handler, ModifyHandler},
            input::Input,
            util::Either,
        },
        http::{header::HeaderValue, Method, Response},
    };

    #[derive(Debug, Clone)]
    pub struct DefaultOptions(pub(super) ());

    impl<H> ModifyHandler<H> for DefaultOptions
    where
        H: Handler,
    {
        type Output = Either<Response<()>, H::Output>;
        type Error = H::Error;
        type Handler = DefaultOptionsHandler<H>; // private

        fn modify(&self, inner: H) -> Self::Handler {
            let mut metadata = inner.metadata().clone();
            metadata
                .allowed_methods_mut()
                .extend(Some(http::Method::OPTIONS));

            let allowed_methods_value = metadata.allowed_methods().to_header_value();

            DefaultOptionsHandler {
                inner,
                metadata,
                allowed_methods_value,
            }
        }
    }

    #[allow(missing_debug_implementations)]
    pub struct DefaultOptionsHandler<H> {
        inner: H,
        metadata: Metadata,
        allowed_methods_value: HeaderValue,
    }

    impl<H> Handler for DefaultOptionsHandler<H>
    where
        H: Handler,
    {
        type Output = Either<Response<()>, H::Output>;
        type Error = H::Error;
        type Handle = HandleDefaultOptions<H::Handle>;

        fn handle(&self) -> Self::Handle {
            HandleDefaultOptions {
                inner: self.inner.handle(),
                allowed_methods_value: Some(self.allowed_methods_value.clone()),
            }
        }

        fn metadata(&self) -> Metadata {
            self.metadata.clone()
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
pub fn map_output<F>(f: F) -> MapOutput<F> {
    self::map_output::MapOutput { f }
}

mod map_output {
    use crate::{
        future::{Poll, TryFuture},
        handler::{metadata::Metadata, Handler, ModifyHandler},
        input::Input,
    };

    #[derive(Debug, Clone)]
    pub struct MapOutput<F> {
        pub(super) f: F,
    }

    impl<H, F, T> ModifyHandler<H> for MapOutput<F>
    where
        H: Handler,
        F: Fn(H::Output) -> T + Clone,
    {
        type Output = T;
        type Error = H::Error;
        type Handler = MapOutputHandler<H, F>;

        fn modify(&self, handler: H) -> Self::Handler {
            MapOutputHandler {
                handler,
                f: self.f.clone(),
            }
        }
    }

    #[allow(missing_debug_implementations)]
    pub struct MapOutputHandler<H, F> {
        handler: H,
        f: F,
    }

    impl<H, F, T> Handler for MapOutputHandler<H, F>
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

        fn metadata(&self) -> Metadata {
            self.handler.metadata()
        }
    }

    #[allow(missing_debug_implementations)]
    pub struct HandleMapOutput<H, F> {
        handle: H,
        f: F,
    }

    #[allow(clippy::redundant_closure)]
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
