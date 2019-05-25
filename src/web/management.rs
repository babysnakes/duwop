use super::errors::*;
use crate::state::AppState;
use std::sync::{Arc, RwLock};

use failure::{format_err, Error};
use futures::future::Future;
use http::StatusCode;
use hyper::{header, Body, Request, Response};
use log::info;
use serde_json;

type ApiResponse = dyn Future<Item = Response<Body>, Error = Error> + Send;

const API_V1_BASE: &str = "/api/v1";
const API_V1_STATE: &str = "/api/v1/state";

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
        (&hyper::Method::GET, API_V1_STATE) => return_state(Arc::clone(&state)),
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

#[cfg(test)]
mod tests {
    use crate::state::ServiceType;
    use crate::web::tests::*;

    use std::collections::HashMap;
    use std::sync::Arc;

    use futures::future::Future;
    use hyper::{rt::Stream, Body, Method, Request, StatusCode};

    #[test]
    fn handle_management_returns_state_if_requested() {
        let state = construct_state(vec![(
            "project-dir".to_string(),
            ServiceType::StaticFiles("/some/path".to_string()),
        )]);
        let services = &state.read().unwrap().services;
        let request = Request::builder()
            .header("host", "duwop.test")
            .method(Method::GET)
            .uri("/api/v1/state")
            .body(Body::empty())
            .unwrap();
        let response = test_web_service(request, Arc::clone(&state)).unwrap();
        assert_eq!(response.status(), StatusCode::OK);
        let _ = response.into_body().concat2().map(|chunk| {
            let body = chunk.iter().map(|b| b.to_owned()).collect::<Vec<u8>>();
            let data: HashMap<String, ServiceType> = serde_json::from_slice(&body[..]).unwrap();
            assert_eq!(data, *services);
        });
    }
}
