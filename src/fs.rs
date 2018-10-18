//! The basic components for serving static files.

use std::borrow::Cow;
use std::fs::{File, Metadata};
use std::io::{self, Read as _Read};
use std::path::PathBuf;
use std::str::FromStr;
use std::time::Duration;
use std::{cmp, fmt, mem};

use bytes::{BufMut, Bytes, BytesMut};
use failure;
use filetime::FileTime;
use futures::{Async, Future, Poll, Stream};
use http::header::HeaderMap;
use http::{header, Response, StatusCode};
use time::{self, Timespec};

use crate::error::Failure;
use crate::input::Input;
use crate::output::{Responder, ResponseBody};
use crate::server::blocking;

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
    fn from_metadata(metadata: &Metadata) -> ETag {
        let last_modified = FileTime::from_last_modification_time(&metadata);
        ETag {
            weak: true,
            tag: format!(
                "{:x}-{:x}.{:x}",
                metadata.len(),
                last_modified.seconds(),
                last_modified.nanoseconds()
            ),
        }
    }

    fn parse_inner(weak: bool, s: &str) -> Result<ETag, failure::Error> {
        if s.len() < 2 {
            bail!("");
        }
        if !s.starts_with('"') || !s.ends_with('"') {
            bail!("");
        }

        let tag = &s[1..s.len() - 1];
        if !tag.is_ascii() {
            bail!("");
        }

        Ok(ETag {
            weak,
            tag: tag.to_owned(),
        })
    }

    fn eq(&self, other: &ETag) -> bool {
        self.tag == other.tag && (self.weak || !other.weak)
    }
}

impl FromStr for ETag {
    type Err = failure::Error;

    fn from_str(s: &str) -> Result<Self, Self::Err> {
        match s.get(0..3) {
            Some("W/\"") if s[2..].starts_with('"') => ETag::parse_inner(true, &s[2..]),
            Some(t) if t.starts_with('"') => ETag::parse_inner(false, s),
            Some(..) => bail!(""),
            None => bail!(""),
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
#[derive(Debug, Default)]
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
    /// # use tsukuyomi::input::Input;
    /// # use tsukuyomi::output::AsyncResponder;
    /// # use tsukuyomi::fs::NamedFile;
    /// #
    /// # #[allow(dead_code)]
    /// fn handler(_: &mut Input) -> impl AsyncResponder {
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
    /// # use tsukuyomi::input::Input;
    /// # use tsukuyomi::output::AsyncResponder;
    /// # use tsukuyomi::fs::{NamedFile, OpenConfig};
    /// use std::time::Duration;
    ///
    /// # #[allow(dead_code)]
    /// fn handler(_: &mut Input) -> impl AsyncResponder {
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
                .body(Default::default())
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

impl Future for OpenFuture {
    type Item = NamedFile;
    type Error = io::Error;

    fn poll(&mut self) -> Poll<Self::Item, Self::Error> {
        let (file, meta) = try_ready!(blocking_io(|| {
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
    fn new(file: File, meta: Metadata, buf_size: Option<usize>) -> ReadStream {
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

                    let buf = try_ready!(blocking_io(|| {
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
const DEFAULT_BUF_SIZE: usize = 8192;

fn blocking_io<T>(f: impl FnOnce() -> io::Result<T>) -> Poll<T, io::Error> {
    match blocking(f) {
        Ok(Async::Ready(ready)) => ready.map(Async::Ready),
        Ok(Async::NotReady) => Ok(Async::NotReady),
        Err(e) => Err(io::Error::new(io::ErrorKind::Other, e)),
    }
}
fn finalize_block_size(buf_size: Option<usize>, meta: &Metadata) -> usize {
    match buf_size {
        Some(n) => cmp::min(meta.len() as usize, n),
        None => cmp::min(meta.len() as usize, block_size(&meta)),
    }
}

#[cfg(unix)]
fn block_size(meta: &Metadata) -> usize {
    use std::os::unix::fs::MetadataExt;
    meta.blksize() as usize
}

#[cfg(not(unix))]
fn block_size(_: &Metadata) -> usize {
    DEFAULT_BUF_SIZE
}
