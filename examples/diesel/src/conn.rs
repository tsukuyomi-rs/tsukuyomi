use diesel::r2d2::{ConnectionManager, Pool, PooledConnection};
use diesel::sqlite::SqliteConnection;
use failure::Error;
use futures::future::poll_fn;
use futures::{Async, Future};

use tsukuyomi::server::blocking::blocking;

pub type ConnPool = Pool<ConnectionManager<SqliteConnection>>;
#[allow(dead_code)]
pub type Conn = PooledConnection<ConnectionManager<SqliteConnection>>;

pub fn init_pool(database_url: String) -> Result<ConnPool, Error> {
    let manager = ConnectionManager::<SqliteConnection>::new(database_url);
    let pool = Pool::builder().max_size(15).build(manager)?;
    Ok(pool)
}

pub fn get_conn(pool: &ConnPool) -> impl Future<Item = Conn, Error = Error> + Send + 'static {
    let pool = pool.clone();
    run_blocking(move || pool.get())
}

pub fn run_blocking<F, T, E>(f: F) -> impl Future<Item = T, Error = Error>
where
    F: FnOnce() -> Result<T, E>,
    E: Into<Error>,
{
    let mut f_opt = Some(f);
    poll_fn(move || match blocking(|| f_opt.take().unwrap()()) {
        Ok(Async::Ready(Ok(v))) => Ok(Async::Ready(v)),
        Ok(Async::Ready(Err(e))) => Err(e.into()),
        Ok(Async::NotReady) => return Ok(Async::NotReady),
        Err(e) => Err(e.into()),
    })
}
