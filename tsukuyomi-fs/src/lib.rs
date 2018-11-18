//! The basic components for serving static files.

#![doc(html_root_url = "https://docs.rs/tsukuyomi-fs/0.1.0")]
#![warn(
    missing_debug_implementations,
    nonstandard_style,
    rust_2018_idioms,
    rust_2018_compatibility,
    unused
)]
#![cfg_attr(tsukuyomi_deny_warnings, deny(warnings))]
#![cfg_attr(tsukuyomi_deny_warnings, doc(test(attr(deny(warnings)))))]
#![cfg_attr(feature = "cargo-clippy", warn(pedantic))]

extern crate futures;
extern crate tsukuyomi;

use {
    futures::Future,
    std::{
        fs, io,
        path::{Path, PathBuf},
        sync::Arc,
    },
    tsukuyomi::{
        app::{self, scope},
        fs::{NamedFile, OpenConfig},
    },
};

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

impl std::ops::Deref for ArcPath {
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

impl<P> scope::Scope for Staticfiles<P>
where
    P: AsRef<Path>,
{
    type Error = app::Error;

    fn configure(self, cx: &mut scope::Context<'_>) -> app::Result<()> {
        let Self { root_dir, config } = self;

        for entry in fs::read_dir(root_dir)? {
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
                    tsukuyomi::app::route() //
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
                    tsukuyomi::app::route()
                        .uri(uri)
                        .with(tsukuyomi::extractor::param::wildcard())
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
