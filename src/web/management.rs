use super::errors::*;
use crate::state::AppState;
use std::sync::{Arc, RwLock};

use failure::{format_err, Error};
use futures::future::Future;
use http::StatusCode;
use hyper::{Body, Request, Response, header};
use log::info;
use serde_json;

type ApiResponse = dyn Future<Item = Response<Body>, Error = Error> + Send;

const API_V1_BASE: &str = "/api/v1";
const API_STATE_PATH: &str = "/api/v1/state";

pub fn handle_management(req: Request<Body>, state: Arc<RwLock<AppState>>) -> Box<ApiResponse> {
    info!("Received {} request: {}", &req.method(), &req.uri().path());
    let response_result = if req.uri().path().starts_with(API_V1_BASE) {
        handle_api_v1_request(req, state)
    } else {
        handle_management_request(req, state)
    };
    match response_result {
        Ok(response) => response,
        Err(err) => Box::new(internal_server_error(err)),
    }
}

pub fn handle_api_v1_request(
    req: Request<Body>,
    state: Arc<RwLock<AppState>>,
) -> Result<Box<ApiResponse>, Error> {
    match (req.method(), req.uri().path()) {
        (&hyper::Method::GET, API_STATE_PATH) => return_state(Arc::clone(&state)),
        _ => Err(format_err!("api not yet implemented")),
    }
}

pub fn handle_management_request(
    _req: Request<Body>,
    _state: Arc<RwLock<AppState>>,
) -> Result<Box<ApiResponse>, Error> {
    Err(format_err!("management not yet implemented"))
}

fn return_state(state: Arc<RwLock<AppState>>) -> Result<Box<ApiResponse>, Error> {
    let unlocked_state = state.try_read().map_err(|e| format_err!("{}", &e))?;
    let data = serde_json::to_string(&unlocked_state.services)?;
    let body = Body::from(data);
    Ok(Box::new(
        futures::future::result(
            Response::builder()
                .status(StatusCode::OK)
                .header(header::CONTENT_TYPE, "application/json")
                .body(body),
        )
        .map_err(Error::from),
    ))
}
