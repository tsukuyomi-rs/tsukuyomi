use bytes::{Bytes, BytesMut};
use futures::{Future, IntoFuture};
use std::fmt;
use std::path::Path;
use std::process::{Command, Stdio};
use tokio_io;
use tokio_process::CommandExt;

use tsukuyomi::error::Error;

#[derive(Debug)]
pub struct Repository<'p> {
    path: &'p Path,
}

impl<'p> Repository<'p> {
    pub fn new<P>(path: &'p P) -> Repository<'p>
    where
        P: AsRef<Path> + ?Sized,
    {
        Repository { path: path.as_ref() }
    }

    pub fn stateless_rpc<'a>(&'a self, mode: RpcMode) -> StatelessRpc<'a> {
        StatelessRpc { repository: self, mode }
    }
}

#[derive(Debug, Copy, Clone, PartialEq)]
pub enum RpcMode {
    Receive,
    Upload,
}

impl RpcMode {
    pub fn as_str(&self) -> &str {
        match *self {
            RpcMode::Receive => "git-receive-pack",
            RpcMode::Upload => "git-upload-pack",
        }
    }
}

impl fmt::Display for RpcMode {
    fn fmt(&self, f: &mut fmt::Formatter) -> fmt::Result {
        f.write_str(self.as_str())
    }
}

#[derive(Debug)]
pub struct StatelessRpc<'a> {
    repository: &'a Repository<'a>,
    mode: RpcMode,
}

impl<'a> StatelessRpc<'a> {
    fn command(&self) -> Command {
        let mut command = Command::new(format!("/usr/lib/git-core/{}", self.mode.as_str()));
        command
            .args(&["--stateless-rpc", "."])
            .current_dir(self.repository.path)
            .stdin(Stdio::null())
            .stdout(Stdio::piped())
            .stderr(Stdio::null());
        command
    }

    pub fn advertise_refs(&self) -> impl Future<Item = Bytes, Error = Error> + Send + 'static {
        let mode = self.mode;
        self.command()
            .arg("--advertise-refs")
            .output_async()
            .map_err(Error::internal_server_error)
            .map(move |output| {
                use std::fmt::Write;
                let prefix_len = mode.as_str().len() + 19;
                let mut body = BytesMut::with_capacity(prefix_len + output.stdout.len());
                let _ = write!(&mut body, "{:0>4x}# service={}\n0000", mode.as_str().len() + 15, mode);
                body.extend_from_slice(&output.stdout);
                body.freeze()
            })
    }

    pub fn call<Bd>(&self, body: Bd) -> impl Future<Item = Bytes, Error = Error> + Send + 'static
    where
        Bd: Future + Send + 'static,
        Bd::Item: AsRef<[u8]> + Send + 'static,
        Bd::Error: Into<Error>,
    {
        self.command()
            .stdin(Stdio::piped())
            .spawn_async()
            .into_future()
            .map_err(Error::internal_server_error)
            .and_then(|mut child| {
                let stdin = child
                    .stdin()
                    .take()
                    .ok_or_else(|| Error::internal_server_error(format_err!("The instance of ChildStdin is not exist")))
                    .into_future();

                stdin.and_then(|stdin| {
                    body.map_err(Into::into)
                        .and_then(move |buf| tokio_io::io::write_all(stdin, buf).map_err(Error::internal_server_error))
                        .and_then(move |_| child.wait_with_output().map_err(Error::internal_server_error))
                        .map(|output| Bytes::from(output.stdout))
                })
            })
    }
}
