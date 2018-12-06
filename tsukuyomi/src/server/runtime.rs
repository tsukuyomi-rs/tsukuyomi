use {super::CritError, futures01::Future, std::io};

pub trait Runtime<F> {
    type Error: Into<CritError>;
    fn run(self, future: F) -> Result<(), Self::Error>;
}

impl<F> Runtime<F> for tokio::runtime::Runtime
where
    F: Future<Item = (), Error = ()> + Send + 'static,
{
    type Error = io::Error;
    fn run(mut self, future: F) -> io::Result<()> {
        self.spawn(future);
        self.shutdown_on_idle().wait().unwrap();
        Ok(())
    }
}

impl<F> Runtime<F> for tokio::runtime::current_thread::Runtime
where
    F: Future<Item = (), Error = ()>,
{
    type Error = io::Error;
    fn run(mut self, future: F) -> io::Result<()> {
        let _ = self.block_on(future);
        Self::run(&mut self).map_err(|e| io::Error::new(io::ErrorKind::Other, e))?;
        Ok(())
    }
}
