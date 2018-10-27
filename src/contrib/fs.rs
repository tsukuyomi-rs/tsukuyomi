//! The basic components for serving static files.

use std::borrow::Cow;
use std::fs::{File, Metadata};
use std::io;
use std::io::Read as _Read;
use std::path::{Path, PathBuf};
use std::str::FromStr;
use std::time::Duration;
use std::{cmp, fmt, mem};

use bytes::{BufMut, Bytes, BytesMut};
use failure;
use failure::Fallible;
use filetime::FileTime;
use futures::{Async, Future, Poll, Stream};
use http::header::HeaderMap;
use http::{header, Response, StatusCode};
use log::trace;
use time::Timespec;
use walkdir::{FilterEntry, WalkDir};

#[doc(no_inline)]
pub use walkdir::DirEntry;

use crate::app::builder::{Scope, ScopeConfig};
use crate::error::{Error, Failure};
use crate::input::Input;
use crate::output::{Output, Responder, ResponseBody};
use crate::server::rt::blocking;

// ==== headers ====

fn parse_http_date(s: &str) -> Result<Timespec, time::ParseError> {
    time::strptime(s, "%a, %d %b %Y %T %Z")
        .or_else(|_| time::strptime(s, "%A, %d-%b-%y %T %Z"))
        .or_else(|_| time::strptime(s, "%c"))
        .map(|tm| tm.to_timespec())
}

#[derive(Debug)]
struct ETag {
    weak: bool,
    tag: String,
}

impl ETag {
    fn from_metadata(metadata: &Metadata) -> Self {
        let last_modified = FileTime::from_last_modification_time(&metadata);
        Self {
            weak: true,
            tag: format!(
                "{:x}-{:x}.{:x}",
                metadata.len(),
                last_modified.seconds(),
                last_modified.nanoseconds()
            ),
        }
    }

    fn parse_inner(weak: bool, s: &str) -> Result<Self, failure::Error> {
        if s.len() < 2 {
            failure::bail!("");
        }
        if !s.starts_with('"') || !s.ends_with('"') {
            failure::bail!("");
        }

        let tag = &s[1..s.len() - 1];
        if !tag.is_ascii() {
            failure::bail!("");
        }

        Ok(Self {
            weak,
            tag: tag.to_owned(),
        })
    }

    fn eq(&self, other: &Self) -> bool {
        self.tag == other.tag && (self.weak || !other.weak)
    }
}

impl FromStr for ETag {
    type Err = failure::Error;

    fn from_str(s: &str) -> Result<Self, Self::Err> {
        match s.get(0..3) {
            Some("W/\"") if s[2..].starts_with('"') => Self::parse_inner(true, &s[2..]),
            Some(t) if t.starts_with('"') => Self::parse_inner(false, s),
            Some(..) => failure::bail!("invalid string to parse ETag"),
            None => failure::bail!("empty string to parse ETag"),
        }
    }
}

impl fmt::Display for ETag {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        if self.weak {
            f.write_str("W/")?;
        }
        write!(f, "\"{}\"", self.tag)
    }
}

// ==== Config ====

/// A set of configuration used in `NamedFile`.
#[derive(Debug, Default, Clone)]
pub struct OpenConfig {
    /// The size of chunked buffers.
    ///
    /// If `None`, it will be guessed based on the block size on the filesystem.
    pub chunk_size: Option<usize>,

    /// The maximal amount of time to refresh the resource.
    ///
    /// If this field is set, the generated HTTP response will include a "Cache-Control" header
    /// that includes the parameter max-age.
    pub max_age: Option<Duration>,
}

// ==== NamedFile ====

/// An instance of `Responder` for responding a file.
#[derive(Debug)]
pub struct NamedFile {
    file: File,
    meta: Metadata,
    etag: ETag,
    last_modified: FileTime,
    config: OpenConfig,
}

impl NamedFile {
    /// Open a specified file with the default configuration.
    ///
    /// # Examples
    ///
    /// ```
    /// # extern crate futures;
    /// # extern crate tsukuyomi;
    /// # use futures::prelude::*;
    /// # use tsukuyomi::input::Input;
    /// # use tsukuyomi::contrib::fs::NamedFile;
    /// #
    /// # #[allow(dead_code)]
    /// fn handler(_: &mut Input) -> impl Future<Error = std::io::Error, Item = NamedFile> {
    ///     NamedFile::open("/path/to/index.html")
    /// }
    /// ```
    pub fn open(path: impl Into<PathBuf>) -> OpenFuture {
        OpenFuture {
            path: path.into(),
            config: None,
        }
    }

    /// Open a specified file with the provided configuration.
    ///
    /// # Examples
    ///
    /// ```
    /// # extern crate futures;
    /// # extern crate tsukuyomi;
    /// # use futures::prelude::*;
    /// # use tsukuyomi::input::Input;
    /// # use tsukuyomi::contrib::fs::{NamedFile, OpenConfig};
    /// use std::time::Duration;
    ///
    /// # #[allow(dead_code)]
    /// fn handler(_: &mut Input) -> impl Future<Error = std::io::Error, Item = NamedFile> {
    ///     NamedFile::open_with_config(
    ///         "/path/to/index.html",
    ///         OpenConfig {
    ///             max_age: Some(Duration::from_secs(60*60*24*10)),
    ///             ..Default::default()
    ///         })
    /// }
    /// ```
    pub fn open_with_config(path: impl Into<PathBuf>, config: OpenConfig) -> OpenFuture {
        OpenFuture {
            path: path.into(),
            config: Some(config),
        }
    }

    #[cfg_attr(feature = "cargo-clippy", allow(cast_sign_loss))]
    fn is_modified(&self, headers: &HeaderMap) -> Result<bool, Failure> {
        if let Some(h) = headers.get(header::IF_NONE_MATCH) {
            trace!("NamedFile::is_modified(): validate If-None-Match");

            let etag: ETag = h
                .to_str()
                .map_err(Failure::bad_request)?
                .parse()
                .map_err(Failure::bad_request)?;
            let modified = !etag.eq(&self.etag);

            trace!(
                "--> self.etag={:?}, etag={:?}, modified={}",
                self.etag,
                etag,
                modified
            );
            return Ok(modified);
        }

        if let Some(h) = headers.get(header::IF_MODIFIED_SINCE) {
            trace!("NamedFile::is_modified(): validate If-Modified-Since");

            let if_modified_since = {
                let timespec = parse_http_date(h.to_str().map_err(Failure::bad_request)?)
                    .map_err(Failure::bad_request)?;
                FileTime::from_unix_time(timespec.sec, timespec.nsec as u32)
            };
            let modified = self.last_modified > if_modified_since;

            trace!(
                "--> if_modified_sicne={:?}, modified={}",
                if_modified_since,
                modified
            );
            return Ok(modified);
        }

        Ok(true)
    }

    fn cache_control(&self) -> Cow<'static, str> {
        match self.config.max_age {
            Some(ref max_age) => format!("public, max-age={}", max_age.as_secs()).into(),
            None => "public".into(),
        }
    }

    #[cfg_attr(feature = "cargo-clippy", allow(cast_possible_wrap))]
    fn last_modified(&self) -> Result<String, time::ParseError> {
        let tm = time::at(Timespec::new(
            self.last_modified.seconds(),
            self.last_modified.nanoseconds() as i32,
        ));
        time::strftime("%c", &tm)
    }
}

impl Responder for NamedFile {
    type Body = ResponseBody;
    type Error = Failure;

    fn respond_to(self, input: &mut Input<'_>) -> Result<Response<Self::Body>, Self::Error> {
        trace!("NamedFile::respond_to");

        if !self.is_modified(input.headers())? {
            return Ok(Response::builder()
                .status(StatusCode::NOT_MODIFIED)
                .body(ResponseBody::empty())
                .unwrap());
        }

        // FIXME: optimize

        let cache_control = self.cache_control();
        let last_modified = self
            .last_modified()
            .map_err(Failure::internal_server_error)?;
        let stream = ReadStream::new(self.file, self.meta, self.config.chunk_size);

        Ok(Response::builder()
            .header(header::CACHE_CONTROL, &*cache_control)
            .header(header::LAST_MODIFIED, &*last_modified)
            .header(header::ETAG, &*self.etag.to_string())
            .body(ResponseBody::wrap_stream(stream))
            .unwrap())
    }
}

// ==== OpenFuture ====

/// A future waiting for opening the file.
#[derive(Debug)]
pub struct OpenFuture {
    path: PathBuf,
    config: Option<OpenConfig>,
}

impl OpenFuture {
    fn poll_respond(&mut self, input: &mut Input<'_>) -> Poll<Output, Error> {
        futures::try_ready!(self.poll())
            .respond_to(input)
            .map(|response| Async::Ready(response.map(Into::into)))
            .map_err(Into::into)
    }
}

impl Future for OpenFuture {
    type Item = NamedFile;
    type Error = io::Error;

    fn poll(&mut self) -> Poll<Self::Item, Self::Error> {
        let (file, meta) = futures::try_ready!(blocking_io(|| {
            let file = File::open(&self.path)?;
            let meta = file.metadata()?;
            Ok((file, meta))
        }));

        let config = self.config.take().unwrap_or_default();

        let last_modified = FileTime::from_last_modification_time(&meta);
        let etag = ETag::from_metadata(&meta);

        Ok(Async::Ready(NamedFile {
            file,
            meta,
            last_modified,
            etag,
            config,
        }))
    }
}

// ==== ReadStream ====

#[derive(Debug)]
struct ReadStream(State);

#[derive(Debug)]
enum State {
    Reading { file: File, buf_size: usize },
    Eof,
    Gone,
}

impl ReadStream {
    fn new(file: File, meta: Metadata, buf_size: Option<usize>) -> Self {
        let buf_size = finalize_block_size(buf_size, &meta);
        drop(meta);
        ReadStream(State::Reading { file, buf_size })
    }
}

impl Stream for ReadStream {
    type Item = Bytes;
    type Error = io::Error;

    fn poll(&mut self) -> Poll<Option<Self::Item>, Self::Error> {
        loop {
            match self.0 {
                State::Reading {
                    ref mut file,
                    buf_size,
                    ..
                } => {
                    trace!("ReadStream::poll(): polling on the mode State::Reading");

                    let buf = futures::try_ready!(blocking_io(|| {
                        let mut buf = BytesMut::with_capacity(buf_size);
                        if !buf.has_remaining_mut() {
                            buf.reserve(buf_size);
                        }
                        unsafe {
                            let n = file.read(buf.bytes_mut())?;
                            buf.advance_mut(n);
                        }
                        Ok(buf)
                    }));

                    if !buf.is_empty() {
                        return Ok(Async::Ready(Some(buf.freeze())));
                    }
                }
                State::Eof => {
                    trace!("ReadStream::poll(): polling on the mode State::Reading");
                    return Ok(Async::Ready(None));
                }
                State::Gone => panic!("unexpected state"),
            };

            match mem::replace(&mut self.0, State::Gone) {
                State::Reading { .. } => self.0 = State::Eof,
                _ => unreachable!("unexpected state"),
            }
        }
    }
}

#[allow(dead_code)]
const DEFAULT_BUF_SIZE: u64 = 8192;

fn blocking_io<T>(f: impl FnOnce() -> io::Result<T>) -> Poll<T, io::Error> {
    match blocking(f) {
        Ok(Async::Ready(ready)) => ready.map(Async::Ready),
        Ok(Async::NotReady) => Ok(Async::NotReady),
        Err(e) => Err(io::Error::new(io::ErrorKind::Other, e)),
    }
}

// FIXME: replace usize to u64
#[cfg_attr(feature = "cargo-clippy", allow(cast_possible_truncation))]
fn finalize_block_size(buf_size: Option<usize>, meta: &Metadata) -> usize {
    match buf_size {
        Some(n) => cmp::min(meta.len(), n as u64) as usize,
        None => cmp::min(meta.len(), block_size(&meta)) as usize,
    }
}

#[cfg(unix)]
fn block_size(meta: &Metadata) -> u64 {
    use std::os::unix::fs::MetadataExt;
    meta.blksize()
}

#[cfg(not(unix))]
fn block_size(_: &Metadata) -> u64 {
    DEFAULT_BUF_SIZE
}

// ==== Staticfiles ====

/// A configuration type for adding entries in the directory to the route.
///
/// This type scans all files under the specified directory by using [`walkdir`],
/// and creates a handler that returns `NamedFile` for each file.
///
/// [`walkdir`]: https://docs.rs/walkdir/2
///
/// # Example
///
/// ```no_run
/// # use tsukuyomi::app::App;
/// use tsukuyomi::contrib::fs::Staticfiles;
///
/// # fn main() -> tsukuyomi::app::AppResult<()> {
/// let assets = Staticfiles::new("./public")
///     .follow_links(true)
///     .same_file_system(false)
///     .filter_entry(|entry| {
///         entry.file_name()
///             .to_str()
///             .map(|s| s.starts_with('.'))
///             .unwrap_or(false)
///     });
///
/// let app = App::builder()
///     .scope(assets)
///     .finish()?;
/// # drop(app);
/// # Ok(())
/// # }
/// ```
#[derive(Debug)]
pub struct Staticfiles<W = WalkDir> {
    walkdir: W,
    config: Option<OpenConfig>,
}

#[cfg_attr(feature = "cargo-clippy", allow(use_self))]
impl Staticfiles {
    /// Create a new `Staticfiles` with the specified directory path.
    pub fn new(root_dir: impl AsRef<Path>) -> Self {
        Self {
            walkdir: WalkDir::new(root_dir).min_depth(1),
            config: None,
        }
    }

    /// Sets the mininum depth of entries.
    ///
    /// See the documentation of [`WalkDir::min_depth`] for details.
    ///
    /// [`WalkDir::min_depth`]: https://docs.rs/walkdir/2/walkdir/struct.WalkDir.html#method.max_depth
    pub fn min_depth(self, depth: usize) -> Self {
        Self {
            walkdir: self.walkdir.min_depth(depth),
            ..self
        }
    }

    /// Sets the maximum depth of entries.
    ///
    /// See the documentation of [`WalkDir::max_depth`] for details.
    ///
    /// [`WalkDir::max_depth`]: https://docs.rs/walkdir/2/walkdir/struct.WalkDir.html#method.max_depth
    pub fn max_depth(self, depth: usize) -> Self {
        Self {
            walkdir: self.walkdir.max_depth(depth),
            ..self
        }
    }

    /// Sets whether to follow symbolic links or not.
    ///
    /// See the documentation of [`WalkDir::follow_links`] for details.
    ///
    /// [`WalkDir::follow_links`]: https://docs.rs/walkdir/2/walkdir/struct.WalkDir.html#method.follow_links
    pub fn follow_links(self, yes: bool) -> Self {
        Self {
            walkdir: self.walkdir.follow_links(yes),
            ..self
        }
    }

    /// Sets whether to cross file system boundaries or not.
    ///
    /// See the documentation of [`WalkDir::same_file_system`] for details.
    ///
    /// [`WalkDir::same_file_system`]: https://docs.rs/walkdir/2/walkdir/struct.WalkDir.html#method.same_file_system
    pub fn same_file_system(self, yes: bool) -> Self {
        Self {
            walkdir: self.walkdir.same_file_system(yes),
            ..self
        }
    }

    /// Sets the predicate whether to skip the entry or not.
    ///
    /// See [the documentation in `walkdir`][filter-entry] for details.
    ///
    /// [filter-entry]: https://docs.rs/walkdir/2/walkdir/struct.IntoIter.html#method.filter_entry
    pub fn filter_entry<P>(self, predicate: P) -> Staticfiles<FilterEntry<walkdir::IntoIter, P>>
    where
        P: FnMut(&DirEntry) -> bool,
    {
        Staticfiles {
            walkdir: self.walkdir.into_iter().filter_entry(predicate),
            config: self.config,
        }
    }
}

impl<W> Staticfiles<W>
where
    W: IntoIterator<Item = walkdir::Result<DirEntry>>,
{
    /// Sets the value of `OpenConfig` used in handlers.
    pub fn with_config(self, config: OpenConfig) -> Self {
        Self {
            config: Some(config),
            ..self
        }
    }

    fn configure_inner(self, scope: &mut Scope<'_>) -> Fallible<()> {
        let Self { walkdir, config } = self;
        for entry in walkdir {
            let entry = entry?;
            if entry.file_type().is_file() {
                let prefix = format!("/{}", entry.path().display()).replace('\\', "/");
                let path = entry.path().canonicalize()?;

                let config = config.clone();
                scope.route((
                    prefix,
                    crate::handler::raw(move |_| {
                        let mut open_future = if let Some(ref config) = config {
                            NamedFile::open_with_config(path.clone(), config.clone())
                        } else {
                            NamedFile::open(path.clone())
                        };
                        crate::handler::Handle::polling(move |input| {
                            open_future.poll_respond(input)
                        })
                    }),
                ));
            }
        }
        Ok(())
    }
}

impl<W> ScopeConfig for Staticfiles<W>
where
    W: IntoIterator<Item = walkdir::Result<DirEntry>>,
{
    fn configure(self, scope: &mut Scope<'_>) {
        if let Err(err) = self.configure_inner(scope) {
            scope.mark_error(err);
        }
    }
}
