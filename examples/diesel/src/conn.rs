use {
    diesel::{
        r2d2::{ConnectionManager, Pool, PooledConnection},
        sqlite::SqliteConnection,
    },
    failure::Fallible,
    tsukuyomi::{
        extractor::Extractor,
        future::{Compat01, TryFuture},
        rt::Future,
    },
};

pub type Conn = PooledConnection<ConnectionManager<SqliteConnection>>;

pub fn extractor<T>(
    url: T,
) -> Fallible<
    impl Extractor<
        Output = (Conn,), //
        Error = tsukuyomi::Error,
        Extract = impl TryFuture<Ok = (Conn,), Error = tsukuyomi::Error> + Send + 'static,
    >,
>
where
    T: Into<String>,
{
    let manager = ConnectionManager::<SqliteConnection>::new(url);
    let pool = Pool::builder().max_size(15).build(manager)?;

    Ok(tsukuyomi::extractor::extract(move || {
        let pool = pool.clone();
        Compat01::from(
            tsukuyomi::rt::blocking(move || pool.get()) //
                .then(|result| {
                    result
                        .map_err(tsukuyomi::error::internal_server_error) // <-- BlockingError
                        .and_then(|result| {
                            result
                                .map(|conn| (conn,))
                                .map_err(tsukuyomi::error::internal_server_error) // <-- r2d2::Error
                        })
                }),
        )
    }))
}
