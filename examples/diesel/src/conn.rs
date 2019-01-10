use {
    diesel::{
        r2d2::{ConnectionManager, Pool, PooledConnection},
        sqlite::SqliteConnection,
    },
    failure::Fallible,
    futures::Future,
    tsukuyomi::{
        extractor::Extractor,
        future::{Futures01CompatExt, TryFuture},
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
        izanami::rt::blocking(move || pool.get()) //
            .then(|result| {
                result
                    .map_err(tsukuyomi::error::internal_server_error) // <-- BlockingError
                    .and_then(|result| {
                        result
                            .map(|conn| (conn,))
                            .map_err(tsukuyomi::error::internal_server_error) // <-- r2d2::Error
                    })
            })
            .compat01()
    }))
}
