#[macro_export]
macro_rules! try_ready {
    ($e:expr) => (match $e {
        Ok($crate::Async::Ready(t)) => t,
        Ok($crate::Async::NotReady) => return Ok($crate::Async::NotReady),
        Err(e) => return Err(From::from(e)),
    })
}