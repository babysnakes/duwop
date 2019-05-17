mod errors;
mod static_files;

use super::state::{AppState, ServiceType};
use errors::*;

use std::net::{Ipv4Addr, SocketAddr};
use std::path::PathBuf;
use std::sync::{Arc, RwLock};

use failure::{format_err, Error};
use futures::future;
use futures::{Future, Poll};
use hyper::header::HOST;
use hyper::{Body, Request, Response, Server};
use hyper_reverse_proxy;

use log::{debug, error, info, trace};

type BoxFut = Box<Future<Item = Response<Body>, Error = hyper::Error> + Send>;
type StaticFuture = Box<Future<Item = Response<Body>, Error = Error> + Send>;
type ErrorFut = Box<Future<Item = Response<Body>, Error = Error> + Send>;

enum MainFuture {
    Static(StaticFuture),
    ReverseProxy(BoxFut),
    ErrorResponse(ErrorFut),
}

impl Future for MainFuture {
    type Item = Response<Body>;
    type Error = Error;

    fn poll(&mut self) -> Poll<Self::Item, Self::Error> {
        match *self {
            MainFuture::Static(ref mut future) => future.poll().map_err(Error::from),
            MainFuture::ReverseProxy(ref mut future) => future.poll().map_err(Error::from),
            MainFuture::ErrorResponse(ref mut future) => future.poll().map_err(Error::from),
        }
    }
}

/// Hyper `Service` implementation that serves all requests.
struct MainService {
    state: Arc<RwLock<AppState>>,
}

impl MainService {
    fn new(state: Arc<RwLock<AppState>>) -> MainService {
        MainService {
            state: Arc::clone(&state),
        }
    }
}

impl hyper::service::Service for MainService {
    type ReqBody = Body;
    type ResBody = Body;
    type Error = Error;
    type Future = MainFuture;

    fn call(&mut self, req: Request<Body>) -> MainFuture {
        let state = self.state.read().unwrap(); // if read state fails we better panic.
        let key = match extract_host(&req) {
            Ok(host) => host,
            Err(e) => return MainFuture::ErrorResponse(Box::new(internal_server_error(e))),
        };
        debug!("received request for key: {}, uri: {}", key, &req.uri());
        match state.services.get(key) {
            None => return MainFuture::ErrorResponse(Box::new(handle_404())),
            Some(service) => match service {
                ServiceType::StaticFiles(path) => {
                    MainFuture::Static(Box::new(static_files::serve(req, &PathBuf::from(path))))
                }
                ServiceType::ReverseProxy(url) => MainFuture::ReverseProxy(
                    hyper_reverse_proxy::call([127, 0, 0, 1].into(), &url.as_str(), req),
                ),
            },
        }
    }
}

pub(super) fn new_server(
    port: u16,
    state: Arc<RwLock<AppState>>,
) -> Box<Future<Item = (), Error = ()> + Send> {
    let addr: SocketAddr = (Ipv4Addr::LOCALHOST, port).into();
    info!("Listening for web requests on {}", &addr);

    Box::new(
        Server::bind(&addr)
            .serve(move || future::ok::<_, Error>(MainService::new(Arc::clone(&state))))
            .map_err(|e| error!("{:?}", e)),
    )
}

fn extract_host<'a>(req: &'a Request<Body>) -> Result<&'a str, Error> {
    trace!("extracting host from headers: {:#?}", req.headers());
    let host_value = match req.headers().get(HOST) {
        Some(host) => host,
        None => return Err(format_err!("Couldn't extract header host")),
    };
    let host_header = host_value.to_str()?;
    let host = host_header.split(":").nth(0).unwrap_or("");
    if host.ends_with(".test") {
        Ok(&host[..host.len() - 5])
    } else {
        Err(format_err!("invalid host domain: {}", &host))
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    use std::collections::HashMap;
    use std::sync::{Arc, RwLock};

    use hyper::{service::Service, Request, StatusCode};

    macro_rules! test_extract_host {
        ($name:ident, $req:expr, $test_fn:expr) => {
            #[test]
            fn $name() {
                let request = $req; // avoid temporary value is freed while still in use error.
                let result = extract_host(&request);
                $test_fn(result);
            }
        };
    }

    #[allow(unused_macros)]
    macro_rules! test_web_service_call {
        ($name:ident, $state:expr, $req:expr, $test_fn:expr) => {
            #[test]
            fn $name() {
                let request = $req;
                let state = $state;
                let mut service = MainService { state: state };
                tokio::run(future::lazy(move || {
                    let response = service.call(request);
                    $test_fn(response.wait());
                    Ok(())
                }))
            }
        };
    }

    fn construct_state(kv: Vec<(String, ServiceType)>) -> Arc<RwLock<AppState>> {
        let mut map = HashMap::new();
        for (k, v) in kv {
            map.insert(k, v);
        }
        let state = AppState { services: map };
        Arc::new(RwLock::new(state))
    }

    test_extract_host! {
        extract_host_extracts_the_host_without_domain,
        Request::builder().header("Host", "example.test").body(Body::empty()).unwrap(),
        |res: Result<&str, Error>| {assert_eq!(res.unwrap(), "example")}
    }

    test_extract_host! {
        extract_host_returns_error_if_no_host_header_is_found,
        Request::builder().body(Body::empty()).unwrap(),
        |res: Result<&str, Error>| {
            assert!(res.is_err(),
            "no host should return error"
        )}
    }

    test_extract_host! {
        extract_host_returns_error_if_domain_is_not_valid,
        Request::builder().header("Host", "example.com").body(Body::empty()).unwrap(),
        |res: Result<&str, Error>| {
            assert!(res.is_err(),
            "invalid domain should return error"
        )}
    }

    test_web_service_call! {
        respond_correctly_to_static_file_request,
        construct_state(vec![
            (
                "project-dir".to_string(),
                ServiceType::StaticFiles(
                    std::env::current_dir().unwrap().into_os_string().into_string().unwrap()
                )),
        ]),
        Request::builder()
        .header("host", "project-dir.test")
        .uri("http://project-dir.test/Cargo.toml")
        .body(Body::empty()).unwrap(),
        |res: Result<Response<Body>, Error>| {
            match res {
                Err(e) => panic!("expected response, got error: {:?}", e),
                Ok(response) => {
                    assert_eq!(response.status(), StatusCode::OK)
                }
            }
        }
    }

    test_web_service_call! {
        request_without_host_should_return_500_error,
        construct_state(vec![
            ("key".to_string(), ServiceType::StaticFiles("/some/path".to_string())),
        ]),
        Request::builder().body(Body::empty()).unwrap(),
        |res: Result<Response<Body>, Error>| {
            match res {
                Err(e) => panic!("expected response, got error: {:?}", e),
                Ok(response) => {
                    assert_eq!(response.status(), StatusCode::INTERNAL_SERVER_ERROR)
                }
            }
        }
    }

    test_web_service_call! {
        request_with_invalid_domain_should_return_500,
        construct_state(vec![
            ("key".to_string(), ServiceType::StaticFiles("/some/path".to_string())),
        ]),
        Request::builder().header("host", "example.com").body(Body::empty()).unwrap(),
        |res: Result<Response<Body>, Error>| {
            match res {
                Err(e) => panic!("expected response, got error: {:?}", e),
                Ok(response) => {
                    assert_eq!(response.status(), StatusCode::INTERNAL_SERVER_ERROR)
                }
            }
        }
    }

    test_web_service_call! {
        request_with_undefined_host_should_return_404,
        construct_state(vec![
            ("key".to_string(), ServiceType::StaticFiles("/some/path".to_string())),
        ]),
        Request::builder().header("host", "undefined.test").body(Body::empty()).unwrap(),
        |res: Result<Response<Body>, Error>| {
            match res {
                Err(e) => panic!("expected response, got error: {:?}", e),
                Ok(response) => {
                    assert_eq!(response.status(), StatusCode::NOT_FOUND)
                }
            }
        }
    }
}
