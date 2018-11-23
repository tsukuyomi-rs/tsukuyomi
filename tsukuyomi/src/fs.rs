//! The basic components for serving static files.

use {
    bytes::{BufMut, Bytes, BytesMut},
    crate::{
        error::Error,
        input::Input,
        output::{Responder, ResponseBody},
        rt::poll_blocking,
    },
    filetime::FileTime,
    futures::{Async, Future, Poll, Stream},
    http::{
        header::{self, HeaderMap},
        Response, StatusCode,
    },
    log::trace,
    std::{
        borrow::Cow,
        cmp, fmt,
        fs::{File, Metadata},
        io::{self, Read as _Read},
        mem,
        ops::Deref,
        path::{Path, PathBuf},
        str::FromStr,
        sync::Arc,
        time::Duration,
    },
    time::Timespec,
};

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
    pub fn open<P>(path: P) -> OpenFuture<P>
    where
        P: AsRef<Path>,
    {
        OpenFuture { path, config: None }
    }

    /// Open a specified file with the provided configuration.
    pub fn open_with_config<P>(path: P, config: OpenConfig) -> OpenFuture<P>
    where
        P: AsRef<Path>,
    {
        OpenFuture {
            path,
            config: Some(config),
        }
    }

    #[cfg_attr(feature = "cargo-clippy", allow(cast_sign_loss))]
    fn is_modified(&self, headers: &HeaderMap) -> Result<bool, Error> {
        if let Some(h) = headers.get(header::IF_NONE_MATCH) {
            trace!("NamedFile::is_modified(): validate If-None-Match");

            let etag: ETag = h
                .to_str()
                .map_err(crate::error::bad_request)?
                .parse()
                .map_err(crate::error::bad_request)?;
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
                let timespec = parse_http_date(h.to_str().map_err(crate::error::bad_request)?)
                    .map_err(crate::error::bad_request)?;
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
    type Error = Error;

    fn respond_to(self, input: &mut Input<'_>) -> Result<Response<Self::Body>, Self::Error> {
        trace!("NamedFile::respond_to");

        if !self.is_modified(input.request.headers())? {
            return Ok(Response::builder()
                .status(StatusCode::NOT_MODIFIED)
                .body(ResponseBody::empty())
                .unwrap());
        }

        // FIXME: optimize

        let cache_control = self.cache_control();
        let last_modified = self
            .last_modified()
            .map_err(crate::error::internal_server_error)?;
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
pub struct OpenFuture<P> {
    path: P,
    config: Option<OpenConfig>,
}

impl<P> Future for OpenFuture<P>
where
    P: AsRef<Path>,
{
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
    match poll_blocking(f) {
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

#[allow(missing_debug_implementations)]
#[derive(Clone)]
struct ArcPath(Arc<PathBuf>);

impl From<PathBuf> for ArcPath {
    fn from(path: PathBuf) -> Self {
        ArcPath(Arc::new(path))
    }
}

impl AsRef<Path> for ArcPath {
    fn as_ref(&self) -> &Path {
        (*self.0).as_ref()
    }
}

impl Deref for ArcPath {
    type Target = Path;

    #[inline]
    fn deref(&self) -> &Self::Target {
        (*self.0).as_ref()
    }
}

/// A configuration type for adding entries in the directory to the route.
#[derive(Debug)]
pub struct Staticfiles<P> {
    root_dir: P,
    config: Option<OpenConfig>,
}

impl<P> Staticfiles<P>
where
    P: AsRef<Path>,
{
    /// Create a new `Staticfiles` with the specified directory path.
    pub fn new(root_dir: P) -> Self {
        Self {
            root_dir,
            config: None,
        }
    }

    /// Sets the value of `OpenConfig` used in handlers.
    pub fn open_config(self, config: OpenConfig) -> Self {
        Self {
            config: Some(config),
            ..self
        }
    }
}

impl<P> crate::app::scope::Scope for Staticfiles<P>
where
    P: AsRef<Path>,
{
    type Error = crate::app::Error;

    fn configure(self, cx: &mut crate::app::scope::Context<'_>) -> crate::app::Result<()> {
        let Self { root_dir, config } = self;

        for entry in std::fs::read_dir(root_dir)? {
            let entry = entry?;
            let file_type = entry.file_type()?;
            let name = entry.file_name();
            let name = name.to_str().ok_or_else(|| {
                io::Error::new(io::ErrorKind::Other, "the filename must be UTF-8")
            })?;
            let path = entry
                .path()
                .canonicalize()
                .map(|path| ArcPath(Arc::new(path)))?;
            let config = config.clone();

            if file_type.is_file() {
                let uri = format!("/{}", name).parse()?;

                cx.add_route(
                    crate::app::route() //
                        .uri(uri)
                        .handle(move || {
                            if let Some(ref config) = config {
                                NamedFile::open_with_config(path.clone(), config.clone())
                                    .map_err(Into::into)
                            } else {
                                NamedFile::open(path.clone()).map_err(Into::into)
                            }
                        }),
                )?;
            } else if file_type.is_dir() {
                let uri = format!("/{}/*path", name).parse()?;
                let root_dir = path;

                cx.add_route(
                    crate::app::route()
                        .uri(uri)
                        .with(crate::extractor::param::wildcard())
                        .handle(move |suffix: PathBuf| {
                            let path = root_dir.join(suffix);
                            if let Some(ref config) = config {
                                NamedFile::open_with_config(path, config.clone())
                                    .map_err(Into::into)
                            } else {
                                NamedFile::open(path).map_err(Into::into)
                            }
                        }),
                )?;
            } else {
                return Err(io::Error::new(io::ErrorKind::Other, "unexpected file type").into());
            }
        }

        Ok(())
    }
}
