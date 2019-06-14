use failure::Error;
use futures::Future;
use http::StatusCode;
use hyper::{header, Body, Response};
use log::warn;

pub fn handle_404() -> impl Future<Item = Response<Body>, Error = Error> {
    futures::future::result(
        Response::builder()
            .header("Content-Type", "text/plain; charset=utf-8")
            .status(StatusCode::NOT_FOUND)
            .body(Body::empty()),
    )
    .map_err(Error::from)
}

pub fn internal_server_error(err: Error) -> impl Future<Item = Response<Body>, Error = Error> {
    warn!("Internal Server Error: {}", err);
    futures::future::result(
        Response::builder()
            .header("Content-Type", "text/plain; charset=utf-8")
            .header(header::CONTENT_LENGTH, 0)
            .status(StatusCode::INTERNAL_SERVER_ERROR)
            .body(Body::empty()),
    )
    .map_err(Error::from)
}

pub fn displayed_error(message: String) -> impl Future<Item = Response<Body>, Error = Error> {
    warn!("displayed error: {}", message);
    futures::future::result(
        Response::builder()
            .header("Content-Type", "text/plain; charset=utf-8")
            .status(StatusCode::INTERNAL_SERVER_ERROR)
            .body(Body::from(message)),
    )
    .map_err(Error::from)
}
