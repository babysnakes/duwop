use super::state::{AppState, ServiceType};

use std::net::{Ipv4Addr, SocketAddr};
use std::sync::{Arc, RwLock};

use failure::Error;
use futures::future::Future;
use hyper::service;
use hyper::{Body, Request, Server};
use hyper::header::HOST;
use hyper_staticfile::Static;

use log::{debug, error, info};

pub(super) fn new_server(port: u16, state: Arc<RwLock<AppState>>) -> Box<Future<Item = (), Error = ()> + Send> {
  let global_service = move || {
    let state = state.clone();
    service::service_fn(move |req: Request<Body>| {
      let app_state = state.read().unwrap(); // TODO: handle errors
      let key = extract_host(&req).unwrap();
      debug!("received request for key: {}, uri: {}", key, &req.uri());

      match app_state.services.get(key) {
        None => unimplemented!(),
        Some(service) => match service {
          ServiceType::StaticFiles(path) => {
            let static_ = Static::new(path);
            static_.serve(req)
          }
        },
      }
    })
  };

  let addr: SocketAddr = (Ipv4Addr::LOCALHOST, port).into();
  info!("Listening for web requests on {}", &addr);

  Box::new(
    Server::bind(&addr)
      .serve(global_service)
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