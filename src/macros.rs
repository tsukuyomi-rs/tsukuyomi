macro_rules! ready {
    ($e:expr) => {{
        use futures::Async;
        match $e {
            Ok(Async::Ready(x)) => Ok(x),
            Ok(Async::NotReady) => return Ok(Async::NotReady),
            Err(e) => Err(e),
        }
    }};
}
