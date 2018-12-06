//! Components for accessing HTTP requests and global/request-local data.

pub use {
    self::body::RequestBody,
    crate::app::imp::{Cookies, Input, Params},
};

use {
    futures01::{Future, IntoFuture, Poll},
    std::{borrow::Cow, cell::Cell, ptr::NonNull, str::Utf8Error},
    url::percent_encoding::percent_decode,
};

#[derive(Debug)]
#[repr(C)]
pub struct PercentEncoded(str);

impl PercentEncoded {
    pub unsafe fn new_unchecked(s: &str) -> &Self {
        &*(s as *const str as *const Self)
    }

    pub fn decode_utf8(&self) -> Result<Cow<'_, str>, Utf8Error> {
        percent_decode(self.0.as_bytes()).decode_utf8()
    }

    pub fn decode_utf8_lossy(&self) -> Cow<'_, str> {
        percent_decode(self.0.as_bytes()).decode_utf8_lossy()
    }
}

pub trait FromPercentEncoded: Sized {
    type Error: Into<crate::Error>;

    fn from_percent_encoded(s: &PercentEncoded) -> Result<Self, Self::Error>;
}

macro_rules! impl_from_percent_encoded {
    ($($t:ty),*) => {$(
        impl FromPercentEncoded for $t {
            type Error = crate::Error;

            #[inline]
            fn from_percent_encoded(s: &PercentEncoded) -> Result<Self, Self::Error> {
                s.decode_utf8()
                    .map_err(crate::error::bad_request)?
                    .parse()
                    .map_err(crate::error::bad_request)
            }
        }
    )*};
}

impl_from_percent_encoded!(bool, char, f32, f64, String);
impl_from_percent_encoded!(i8, i16, i32, i64, i128, isize);
impl_from_percent_encoded!(u8, u16, u32, u64, u128, usize);
impl_from_percent_encoded!(
    std::net::SocketAddr,
    std::net::SocketAddrV4,
    std::net::SocketAddrV6,
    std::net::IpAddr,
    std::net::Ipv4Addr,
    std::net::Ipv6Addr,
    url::Url,
    uuid::Uuid
);

impl FromPercentEncoded for std::path::PathBuf {
    type Error = crate::Error;

    #[inline]
    fn from_percent_encoded(s: &PercentEncoded) -> Result<Self, Self::Error> {
        s.decode_utf8()
            .map(|s| Self::from(s.into_owned()))
            .map_err(crate::error::bad_request)
    }
}

/// Creates a `Future` from the specified closure that process an abritrary asynchronous computation.
pub fn poll_fn<F, T, E>(mut f: F) -> impl Future<Item = T, Error = E>
where
    F: FnMut(&mut Input<'_>) -> Poll<T, E>,
{
    futures01::future::poll_fn(move || with_get_current(|input| f(input)))
}

/// Creates a `Future` which has the same result as the future returned from the specified function.
pub fn lazy<F, R>(f: F) -> impl Future<Item = R::Item, Error = R::Error>
where
    F: FnOnce(&mut Input<'_>) -> R,
    R: IntoFuture,
{
    futures01::future::lazy(move || with_get_current(f))
}

thread_local! {
    static INPUT: Cell<Option<NonNull<Input<'static>>>> = Cell::new(None);
}

#[allow(missing_debug_implementations)]
struct ResetOnDrop(Option<NonNull<Input<'static>>>);

impl Drop for ResetOnDrop {
    fn drop(&mut self) {
        INPUT.with(|input| {
            input.set(self.0.take());
        })
    }
}

/// Returns `true` if the reference to `Input` is set to the current task.
#[inline(always)]
pub fn is_set_current() -> bool {
    INPUT.with(|input| input.get().is_some())
}

impl<'task> Input<'task> {
    /// Stores this reference to the task local storage and executes the specified closure.
    ///
    /// The stored reference to `Input` can be accessed by using `input::with_get_current`.
    #[inline]
    #[allow(clippy::cast_ptr_alignment)]
    pub fn with_set_current<F, R>(&mut self, f: F) -> R
    where
        F: FnOnce() -> R,
    {
        // safety: The value of `self: &mut Input` is always non-null.
        let prev = INPUT.with(|input| {
            let ptr = self as *mut Input<'_> as *mut () as *mut Input<'static>;
            input.replace(Some(unsafe { NonNull::new_unchecked(ptr) }))
        });
        let _reset = ResetOnDrop(prev);
        f()
    }
}

/// Acquires a mutable borrow of `Input` from the current task context and executes the provided
/// closure with its reference.
///
/// # Panics
///
/// This function only work in the management of the framework and causes a panic
/// if any references to `Input` is not set at the current task.
/// Do not use this function outside of futures returned by the handler functions.
/// Such situations often occurs by spawning tasks by the external `Executor`
/// (typically calling `tokio::spawn()`).
///
/// In additional, this function forms a (dynamic) scope to prevent the references to `Input`
/// violate the borrowing rule in Rust.
/// Duplicate borrowings such as the following code are reported as a runtime error.
///
/// ```ignore
/// with_get_current(|input| {
///     some_process()
/// });
///
/// fn some_process() {
///     // Duplicate borrowing of `Input` occurs at this point.
///     with_get_current(|input| { ... })
/// }
/// ```
pub fn with_get_current<R>(f: impl FnOnce(&mut Input<'_>) -> R) -> R {
    let input_ptr = INPUT.with(|input| input.replace(None));
    let _reset = ResetOnDrop(input_ptr);
    let mut input_ptr =
        input_ptr.expect("Any reference to Input are not set at the current task context.");
    // safety: The lifetime of `input_ptr` is always shorter then the borrowing of `Input` in `with_set_current()`
    f(unsafe { input_ptr.as_mut() })
}

/// Components for receiving incoming request bodies.
pub mod body {
    use {
        crate::error::Critical,
        bytes::{Buf, BufMut, Bytes, BytesMut},
        futures01::{Async, Future, Poll, Stream},
        http::header::HeaderMap,
        hyper::body::{Body, Payload},
        std::{io, mem},
    };

    #[derive(Debug)]
    pub struct RequestBody(Body);

    impl RequestBody {
        #[inline]
        pub(crate) fn on_upgrade(self) -> OnUpgrade {
            OnUpgrade(self.0.on_upgrade())
        }

        pub(crate) fn into_inner(self) -> Body {
            self.0
        }

        /// Convert this instance into a `Future` which polls all chunks in the incoming message body
        /// and merges them into a `Bytes`.
        pub fn read_all(self) -> ReadAll {
            ReadAll::new(self)
        }
    }

    impl From<Body> for RequestBody {
        fn from(body: Body) -> Self {
            RequestBody(body)
        }
    }

    impl Payload for RequestBody {
        type Data = io::Cursor<Bytes>;
        type Error = Critical;

        #[inline]
        fn poll_data(&mut self) -> Poll<Option<Self::Data>, Self::Error> {
            self.0
                .poll_data()
                .map(|x| x.map(|data_opt| data_opt.map(|data| io::Cursor::new(data.into_bytes()))))
                .map_err(Critical::new)
        }

        #[inline]
        fn poll_trailers(&mut self) -> Poll<Option<HeaderMap>, Self::Error> {
            self.0.poll_trailers().map_err(Critical::new)
        }

        #[inline]
        fn is_end_stream(&self) -> bool {
            self.0.is_end_stream()
        }

        #[inline]
        fn content_length(&self) -> Option<u64> {
            self.0.content_length()
        }
    }

    impl Stream for RequestBody {
        type Item = io::Cursor<Bytes>;
        type Error = Critical;

        #[inline]
        fn poll(&mut self) -> Poll<Option<Self::Item>, Self::Error> {
            self.poll_data()
        }
    }

    #[cfg(feature = "tower-middleware")]
    mod tower {
        use super::*;

        use tower_web::util::BufStream;

        impl BufStream for RequestBody {
            type Item = hyper::Chunk;
            type Error = hyper::Error;

            #[inline]
            fn poll(&mut self) -> Poll<Option<Self::Item>, Self::Error> {
                BufStream::poll(&mut self.0)
            }

            fn size_hint(&self) -> tower_web::util::buf_stream::SizeHint {
                self.0.size_hint()
            }
        }
    }

    /// An asynchronous I/O upgraded from HTTP connection.
    ///
    /// Currenly, this type is implemented as a thin wrapper of `hyper::upgrade::Upgraded`.
    #[derive(Debug)]
    pub struct UpgradedIo(hyper::upgrade::Upgraded);

    impl io::Read for UpgradedIo {
        #[inline]
        fn read(&mut self, buf: &mut [u8]) -> io::Result<usize> {
            self.0.read(buf)
        }
    }

    impl io::Write for UpgradedIo {
        #[inline]
        fn write(&mut self, buf: &[u8]) -> io::Result<usize> {
            self.0.write(buf)
        }

        #[inline]
        fn flush(&mut self) -> io::Result<()> {
            self.0.flush()
        }
    }

    impl tokio::io::AsyncRead for UpgradedIo {
        #[inline]
        unsafe fn prepare_uninitialized_buffer(&self, buf: &mut [u8]) -> bool {
            tokio::io::AsyncRead::prepare_uninitialized_buffer(&self.0, buf)
        }

        #[inline]
        fn read_buf<B: BufMut>(&mut self, buf: &mut B) -> Poll<usize, io::Error> {
            tokio::io::AsyncRead::read_buf(&mut self.0, buf)
        }
    }

    impl tokio::io::AsyncWrite for UpgradedIo {
        #[inline]
        fn shutdown(&mut self) -> Poll<(), io::Error> {
            tokio::io::AsyncWrite::shutdown(&mut self.0)
        }

        #[inline]
        fn write_buf<B: Buf>(&mut self, buf: &mut B) -> Poll<usize, io::Error> {
            tokio::io::AsyncWrite::write_buf(&mut self.0, buf)
        }
    }

    #[derive(Debug)]
    pub(crate) struct OnUpgrade(hyper::upgrade::OnUpgrade);

    impl Future for OnUpgrade {
        type Item = UpgradedIo;
        type Error = Critical;

        fn poll(&mut self) -> Poll<Self::Item, Self::Error> {
            self.0
                .poll()
                .map(|x| x.map(UpgradedIo))
                .map_err(Critical::new)
        }
    }

    // ==== ReadAll ====

    /// A future to receive the entire of incoming message body.
    #[derive(Debug)]
    #[must_use = "futures do nothing unless polled"]
    pub struct ReadAll {
        state: ReadAllState,
    }

    #[derive(Debug)]
    enum ReadAllState {
        Receiving(RequestBody, BytesMut),
        Done,
    }

    impl ReadAll {
        fn new(body: RequestBody) -> Self {
            Self {
                state: ReadAllState::Receiving(body, BytesMut::new()),
            }
        }
    }

    impl Future for ReadAll {
        type Item = Bytes;
        type Error = Critical;

        fn poll(&mut self) -> Poll<Self::Item, Self::Error> {
            use self::ReadAllState::*;
            loop {
                match self.state {
                    Receiving(ref mut body, ref mut buf) => {
                        while let Some(chunk) = futures01::try_ready!(body.poll_data()) {
                            let chunk = chunk.into_inner();
                            buf.extend_from_slice(&*chunk);
                        }
                    }
                    Done => panic!("cannot resolve twice"),
                }

                match mem::replace(&mut self.state, Done) {
                    Receiving(_body, buf) => {
                        // debug_assert!(body.is_end_stream());
                        return Ok(Async::Ready(buf.freeze()));
                    }
                    Done => unreachable!(),
                }
            }
        }
    }
}
