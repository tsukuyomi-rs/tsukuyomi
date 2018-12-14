use {
    super::Extractor,
    crate::{core::Never, generic::Func, input::Input},
    futures01::Future,
};

#[derive(Debug)]
pub struct ExtractorExt<E>(E);

impl<E> From<E> for ExtractorExt<E>
where
    E: Extractor,
{
    fn from(extractor: E) -> Self {
        Self::new(extractor)
    }
}

impl<E> ExtractorExt<E>
where
    E: Extractor,
{
    /// Creates a `Builder` from the specified extractor.
    #[inline]
    pub fn new(extractor: E) -> Self {
        ExtractorExt(extractor)
    }

    /// Returns the inner extractor.
    #[inline]
    pub fn into_inner(self) -> E {
        self.0
    }

    pub fn optional<T>(self) -> ExtractorExt<impl Extractor<Output = (Option<T>,)>>
    where
        E: Extractor<Output = (T,)>,
        T: Send + 'static,
    {
        ExtractorExt {
            0: super::raw(move |input| {
                self.0
                    .extract(input)
                    .then(|result| Ok::<_, Never>((result.ok().map(|(x,)| x),)))
            }),
        }
    }

    pub fn either_or<T>(self, other: T) -> ExtractorExt<impl Extractor<Output = E::Output>>
    where
        T: Extractor<Output = E::Output>,
        T::Error: 'static,
        E::Output: Send + 'static,
        E::Error: 'static,
    {
        use futures01::future::Either;

        let left = self.0;
        let right = other;
        ExtractorExt {
            0: super::raw(move |input| {
                let left = left.extract(input);
                let right = right.extract(input);
                left.select2(right).then(|result| match result {
                    Ok(Either::A((a, _))) => Either::A(futures01::future::ok(a)),
                    Ok(Either::B((b, _))) => Either::A(futures01::future::ok(b)),
                    Err(Either::A((_, b))) => Either::B(Either::B(b.map_err(Into::into))),
                    Err(Either::B((_, a))) => Either::B(Either::A(a.map_err(Into::into))),
                })
            }),
        }
    }

    pub fn map<F>(self, f: F) -> ExtractorExt<impl Extractor<Output = (F::Out,)>>
    where
        F: Func<E::Output> + Clone + Send + 'static,
    {
        ExtractorExt {
            0: super::raw(move |input| {
                let f = f.clone();
                self.0.extract(input).map(move |args| (f.call(args),))
            }),
        }
    }
}

impl<E> Extractor for ExtractorExt<E>
where
    E: Extractor,
{
    type Output = E::Output;
    type Error = E::Error;
    type Future = E::Future;

    #[inline]
    fn extract(&self, input: &mut Input<'_>) -> Self::Future {
        self.0.extract(input)
    }
}
