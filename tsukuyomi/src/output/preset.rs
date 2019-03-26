use {super::*, std::marker::PhantomData};

/// A trait representing the *preset* for deriving the implementation of `Responder`.
pub trait Preset<T> {
    type Upgrade: Upgrade;
    type Error: Into<Error>;
    type Respond: Respond<Upgrade = Self::Upgrade, Error = Self::Error>;

    fn respond(this: T) -> Self::Respond;
}

#[allow(missing_debug_implementations)]
pub struct Rendered<T, P>(T, PhantomData<P>);

impl<T, P> Rendered<T, P>
where
    P: Preset<T>,
{
    pub fn new(data: T) -> Self {
        Rendered(data, PhantomData)
    }
}

impl<T, P> Responder for Rendered<T, P>
where
    P: Preset<T>,
{
    type Upgrade = P::Upgrade;
    type Error = P::Error;
    type Respond = P::Respond;

    fn respond(self) -> Self::Respond {
        P::respond(self.0)
    }
}

#[allow(missing_debug_implementations)]
pub struct Json(());

mod json {
    use super::*;
    use {
        crate::{
            future::{Poll, TryFuture},
            upgrade::NeverUpgrade,
        },
        serde::Serialize,
    };

    impl<T> Preset<T> for Json
    where
        T: Serialize,
    {
        type Upgrade = NeverUpgrade;
        type Error = Error;
        type Respond = JsonRespond<T>;

        fn respond(this: T) -> Self::Respond {
            JsonRespond(this)
        }
    }

    #[allow(missing_debug_implementations)]
    pub struct JsonRespond<T>(T);

    impl<T> TryFuture for JsonRespond<T>
    where
        T: Serialize,
    {
        type Ok = Response;
        type Error = Error;

        fn poll_ready(&mut self, _: &mut Input<'_>) -> Poll<Self::Ok, Self::Error> {
            let body = serde_json::to_vec(&self.0).map_err(crate::error::internal_server_error)?;
            Ok(crate::output::make_response(body, "application/json").into())
        }
    }
}

#[allow(missing_debug_implementations)]
pub struct JsonPretty(());

mod json_pretty {
    use super::*;
    use {
        crate::{
            future::{Poll, TryFuture},
            upgrade::NeverUpgrade,
        },
        serde::Serialize,
    };

    impl<T> Preset<T> for JsonPretty
    where
        T: Serialize,
    {
        type Upgrade = NeverUpgrade;
        type Error = Error;
        type Respond = JsonPrettyRespond<T>;

        fn respond(this: T) -> Self::Respond {
            JsonPrettyRespond(this)
        }
    }

    #[allow(missing_debug_implementations)]
    pub struct JsonPrettyRespond<T>(T);

    impl<T> TryFuture for JsonPrettyRespond<T>
    where
        T: Serialize,
    {
        type Ok = Response;
        type Error = Error;

        fn poll_ready(&mut self, _: &mut Input<'_>) -> Poll<Self::Ok, Self::Error> {
            let body = serde_json::to_vec_pretty(&self.0) //
                .map_err(crate::error::internal_server_error)?;
            Ok(crate::output::make_response(body, "application/json").into())
        }
    }
}

#[allow(missing_debug_implementations)]
pub struct Html(());

mod html {
    use super::*;
    use crate::{
        future::{Poll, TryFuture},
        upgrade::NeverUpgrade,
    };

    impl<T> Preset<T> for Html
    where
        T: Into<ResponseBody>,
    {
        type Upgrade = NeverUpgrade;
        type Error = Error;
        type Respond = HtmlRespond;

        fn respond(this: T) -> Self::Respond {
            HtmlRespond(Some(this.into()))
        }
    }

    #[allow(missing_debug_implementations)]
    pub struct HtmlRespond(Option<ResponseBody>);

    impl TryFuture for HtmlRespond {
        type Ok = Response;
        type Error = Error;

        fn poll_ready(&mut self, _: &mut Input<'_>) -> Poll<Self::Ok, Self::Error> {
            let body = self.0.take().expect("the future has already been polled.");
            Ok(crate::output::make_response(body, "text/html").into())
        }
    }
}
