mod static_files;

use super::state::{AppState, ServiceType};

use std::net::{Ipv4Addr, SocketAddr};
use std::path::PathBuf;
use std::sync::{Arc, RwLock};

use failure::Error;
use futures::future;
use futures::{Future, Poll};
use hyper::header::HOST;
use hyper::{Body, Request, Response, Server};
use hyper_reverse_proxy;

use log::{debug, error, info};

type BoxFut = Box<Future<Item = Response<Body>, Error = hyper::Error> + Send>;
type StaticFuture = Box<Future<Item = Response<Body>, Error = Error> + Send>;

enum MainFuture {
    Static(StaticFuture),
    ReverseProxy(BoxFut),
}

impl Future for MainFuture {
    type Item = Response<Body>;
    type Error = Error;

    fn poll(&mut self) -> Poll<Self::Item, Self::Error> {
        match *self {
            MainFuture::Static(ref mut future) => future.poll().map_err(Error::from),
            MainFuture::ReverseProxy(ref mut future) => future.poll().map_err(Error::from),
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
        let state = self.state.read().unwrap(); // TODO: handle error
        let key = extract_host(&req).unwrap();
        debug!("received request for key: {}, uri: {}", key, &req.uri());
        match state.services.get(key) {
            None => unimplemented!(),
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
    let host_header = req.headers().get(HOST).unwrap().to_str().unwrap(); // TODO: handle errors
    let host = host_header.split(":").nth(0).unwrap_or("");
    if host.ends_with(".test") {
        Ok(&host[..host.len() - 5])
    } else {
        unreachable!()
    }
}
