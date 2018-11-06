use diesel::r2d2::{ConnectionManager, Pool, PooledConnection};
use diesel::sqlite::SqliteConnection;

pub type Conn = PooledConnection<ConnectionManager<SqliteConnection>>;

pub fn extractor(
    database_url: impl Into<String>,
) -> failure::Fallible<impl tsukuyomi::extractor::Extractor<Output = (Conn,)>> {
    let manager = ConnectionManager::<SqliteConnection>::new(database_url);
    let pool = Pool::builder().max_size(15).build(manager)?;

    Ok(tsukuyomi::extractor(move |_| {
        let pool = pool.clone();
        tsukuyomi::rt::blocking_section(move || {
            pool.get().map_err(tsukuyomi::error::internal_server_error)
        })
    }))
}
