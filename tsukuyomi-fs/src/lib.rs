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
extern crate walkdir;

use futures::Future;
use std::path::Path;
use walkdir::{FilterEntry, WalkDir};

use tsukuyomi::app::scope::{ScopeConfig, ScopeContext};
use tsukuyomi::app::{AppError, AppResult};
use tsukuyomi::fs::{NamedFile, OpenConfig};

#[doc(no_inline)]
pub use walkdir::DirEntry;

/// A configuration type for adding entries in the directory to the route.
///
/// This type scans all files under the specified directory by using [`walkdir`],
/// and creates a handler that returns `NamedFile` for each file.
///
/// [`walkdir`]: https://docs.rs/walkdir/2
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
    pub fn open_config(self, config: OpenConfig) -> Self {
        Self {
            config: Some(config),
            ..self
        }
    }
}

impl<W> ScopeConfig for Staticfiles<W>
where
    W: IntoIterator<Item = walkdir::Result<DirEntry>>,
{
    type Error = AppError;

    fn configure(self, cx: &mut ScopeContext<'_>) -> AppResult<()> {
        let Self { walkdir, config } = self;

        for entry in walkdir {
            let entry = entry?;
            if entry.file_type().is_file() {
                let prefix = format!("/{}", entry.path().display()).replace('\\', "/");
                let path = entry.path().canonicalize()?;

                let config = config.clone();
                cx.route(
                    tsukuyomi::app::route::builder()
                        .uri(prefix.parse()?)
                        .handle(move || {
                            if let Some(ref config) = config {
                                NamedFile::open_with_config(path.clone(), config.clone())
                                    .map_err(Into::into)
                            } else {
                                NamedFile::open(path.clone()).map_err(Into::into)
                            }
                        }),
                )?;
            }
        }

        Ok(())
    }
}
