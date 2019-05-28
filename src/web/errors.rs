use failure::Error;
use futures::Future;
use http::StatusCode;
use hyper::{header, Body, Response};
use log::warn;

pub fn handle_404() -> impl Future<Item = Response<Body>, Error = Error> {
    futures::future::result(
        Response::builder()
            .status(StatusCode::NOT_FOUND)
            .body(Body::empty()),
    )
    .map_err(Error::from)
}

pub fn bad_request() -> impl Future<Item = Response<Body>, Error = Error> {
    futures::future::result(
        Response::builder()
            .status(StatusCode::BAD_REQUEST)
            .body(Body::empty()),
    )
    .map_err(Error::from)
}

pub fn internal_server_error(err: Error) -> impl Future<Item = Response<Body>, Error = Error> {
    warn!("Internal Server Error: {}", err);
    futures::future::result(
        Response::builder()
            .status(StatusCode::INTERNAL_SERVER_ERROR)
            .header(header::CONTENT_LENGTH, 0)
            .body(Body::empty()),
    )
    .map_err(Error::from)
}
