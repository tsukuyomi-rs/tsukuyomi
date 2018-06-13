extern crate futures;
extern crate pretty_env_logger;
extern crate tokio_process;
extern crate tsukuyomi;
#[macro_use]
extern crate failure;
extern crate serde_qs;
#[macro_use]
extern crate serde;
extern crate bytes;
extern crate http;
extern crate tokio_io;

mod git;

use tsukuyomi::app::AppState;
use tsukuyomi::error::HttpError;
use tsukuyomi::output::ResponseBody;
use tsukuyomi::{App, Context, Error};

use std::path::PathBuf;
use std::{env, fs};

use futures::{future, Future};
use http::{header, Response, StatusCode};

use git::{Repository, RpcMode};

#[derive(Debug, Clone)]
struct RepositoryPath(PathBuf);

fn main() -> tsukuyomi::AppResult<()> {
    pretty_env_logger::init();

    let repo_path = env::args().nth(1).ok_or_else(|| format_err!("empty repository path"))?;
    let repo_path = RepositoryPath(fs::canonicalize(repo_path)?);

    let app = App::builder()
        .mount("/", |r| {
            r.get("/info/refs", handle_info_refs);
            r.post("/git-receive-pack", |cx| handle_rpc(cx, RpcMode::Receive));
            r.post("/git-upload-pack", |cx| handle_rpc(cx, RpcMode::Upload));
        })
        .manage(repo_path)
        .finish()?;

    tsukuyomi::run(app)
}

fn handle_info_refs(cx: &Context) -> Box<Future<Item = Response<ResponseBody>, Error = Error> + Send> {
    let mode = match validate_info_refs(cx) {
        Ok(service_name) => service_name,
        Err(err) => return Box::new(future::err(err.into())),
    };

    let repo_path = AppState::with(|s| s.state::<RepositoryPath>().cloned().unwrap());

    let future = Repository::new(&repo_path.0)
        .stateless_rpc(mode)
        .advertise_refs()
        .and_then(move |output| {
            Response::builder()
                .header(header::CACHE_CONTROL, "no-cache")
                .header(header::CONTENT_TYPE, &*format!("application/x-{}-advertisement", mode))
                .body(output.into())
                .map_err(Error::internal_server_error)
        });

    Box::new(future)
}

#[derive(Debug, Deserialize)]
struct InfoRefs {
    service: String,
}

fn validate_info_refs(cx: &Context) -> Result<RpcMode, HandleError> {
    let query = cx.uri().query().ok_or_else(|| HandleError::Forbidden)?;

    let info_refs: InfoRefs = serde_qs::from_str(query).map_err(|cause| HandleError::InvalidQuery {
        cause: failure::SyncFailure::new(cause),
    })?;

    match &*info_refs.service {
        "git-receive-pack" => Ok(RpcMode::Receive),
        "git-upload-pack" => Ok(RpcMode::Upload),
        name => Err(HandleError::InvalidServiceName { name: name.into() }),
    }
}

fn handle_rpc(cx: &Context, mode: RpcMode) -> Box<Future<Item = Response<ResponseBody>, Error = Error> + Send> {
    if let Err(e) = validate_rpc(cx, mode) {
        return Box::new(future::err(e.into()));
    }

    let input = cx.body().read_all().map_err(Error::critical);

    let repo_path = AppState::with(|s| s.state::<RepositoryPath>().cloned().unwrap());

    let future = Repository::new(&repo_path.0)
        .stateless_rpc(mode)
        .call(input)
        .and_then(move |output| {
            Response::builder()
                .header(header::CACHE_CONTROL, "no-cache")
                .header(header::CONTENT_TYPE, &*format!("application/x-{}-result", mode))
                .body(output.into())
                .map_err(Error::internal_server_error)
        });

    Box::new(future)
}

fn validate_rpc(cx: &Context, mode: RpcMode) -> Result<(), HandleError> {
    let content_type = format!("application/x-{}-request", mode);
    match cx.headers().get(header::CONTENT_TYPE) {
        Some(h) if h.as_bytes() == content_type.as_bytes() => Ok(()),
        _ => Err(HandleError::InvalidContentType { mode: mode }),
    }
}

#[derive(Debug, Fail)]
enum HandleError {
    #[fail(
        display = "Dumb protocol is not supported.\n\
                   Please upgrade your Git client.\n"
    )]
    Forbidden,

    #[fail(display = "Invalid query: {}", cause)]
    InvalidQuery {
        cause: failure::SyncFailure<serde_qs::Error>,
    },

    #[fail(display = "`{}' is invalid service name", name)]
    InvalidServiceName { name: String },

    #[fail(display = "Requires that the Content-type is equal to `application/x-{}-request'.\n", mode)]
    InvalidContentType { mode: RpcMode },
}

// FIXME: custom derive
impl HttpError for HandleError {
    fn status_code(&self) -> StatusCode {
        match *self {
            HandleError::Forbidden => StatusCode::FORBIDDEN,
            | HandleError::InvalidQuery { .. }
            | HandleError::InvalidServiceName { .. }
            | HandleError::InvalidContentType { .. } => StatusCode::BAD_REQUEST,
        }
    }
}
