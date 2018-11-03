#![feature(use_extern_macros)]
#![feature(proc_macro_non_items, generators)]

extern crate bytes;
extern crate failure;
extern crate futures_await as futures;
extern crate http;
extern crate pretty_env_logger;
extern crate serde;
extern crate serde_qs;
extern crate tokio_io;
extern crate tokio_process;
extern crate tsukuyomi;

mod git;

use tsukuyomi::error::HttpError;
use tsukuyomi::output::ResponseBody;
use tsukuyomi::{handler, input, App, Error, Input};

use failure::{format_err, Fail};
use futures::prelude::{async, await, Future};
use http::{header, Method, Response, StatusCode};
use serde::Deserialize;
use std::{env, fs};

use git::{Repository, RpcMode};

fn main() -> tsukuyomi::AppResult<()> {
    pretty_env_logger::init();

    let repo_path = env::args().nth(1).ok_or_else(|| format_err!("empty repository path"))?;
    let repo = Repository::new(fs::canonicalize(repo_path)?);

    let app = App::builder()
        .set(repo)
        .route(("/info/refs", handler::wrap_async(|_| handle_info_refs())))
        .route((
            "/git-receive-pack",
            Method::POST,
            handler::wrap_async(|_| handle_rpc(RpcMode::Receive)),
        ))
        .route((
            "/git-upload-pack",
            Method::POST,
            handler::wrap_async(|_| handle_rpc(RpcMode::Upload)),
        ))
        .finish()?;

    tsukuyomi::run(app)
}

#[async]
fn handle_info_refs() -> tsukuyomi::Result<Response<ResponseBody>> {
    let mode = input::with_get_current(|input| validate_info_refs(input))?;

    let advertise_refs =
        input::with_get_current(|input| input.get::<Repository>().unwrap().stateless_rpc(mode).advertise_refs());

    let output = await!(advertise_refs)?;

    Response::builder()
        .header(header::CACHE_CONTROL, "no-cache")
        .header(header::CONTENT_TYPE, &*format!("application/x-{}-advertisement", mode))
        .body(output.into())
        .map_err(Error::internal_server_error)
}

#[derive(Debug, Deserialize)]
struct InfoRefs {
    service: String,
}

fn validate_info_refs(cx: &Input) -> Result<RpcMode, HandleError> {
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

#[async]
fn handle_rpc(mode: RpcMode) -> tsukuyomi::Result<Response<ResponseBody>> {
    input::with_get_current(|input| validate_rpc(input, mode))?;

    let body = input::with_get_current(|input| input.body_mut().read_all().map_err(Error::critical));

    let rpc_call = input::with_get_current(|input| input.get::<Repository>().unwrap().stateless_rpc(mode).call(body));
    let output = await!(rpc_call)?;

    Response::builder()
        .header(header::CACHE_CONTROL, "no-cache")
        .header(header::CONTENT_TYPE, &*format!("application/x-{}-result", mode))
        .body(output.into())
        .map_err(Error::internal_server_error)
}

fn validate_rpc(cx: &Input, mode: RpcMode) -> Result<(), HandleError> {
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
