use diesel::r2d2::{ConnectionManager, Pool, PooledConnection};
use diesel::sqlite::SqliteConnection;
use failure::Fallible;

use tsukuyomi::error::Error;
use tsukuyomi::extractor::{Extractor, ExtractorExt};

pub type Conn = PooledConnection<ConnectionManager<SqliteConnection>>;

pub fn extractor<T>(url: T) -> Fallible<impl Extractor<Output = (Conn,), Error = Error>>
where
    T: Into<String>,
{
    let manager = ConnectionManager::<SqliteConnection>::new(url);
    let pool = Pool::builder().max_size(15).build(manager)?;

    Ok(tsukuyomi::extractor::unit().and_then(move || {
        let pool = pool.clone();
        tsukuyomi::rt::blocking_section(move || {
            pool.get().map_err(tsukuyomi::error::internal_server_error)
        })
    }))
}
