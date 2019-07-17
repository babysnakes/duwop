use super::errors::*;

use std::net::SocketAddr;

use failure::Error;
use futures::{future::Either, Future};
use http::status::StatusCode;
use hyper::client::HttpConnector;
use hyper::header::{HeaderName, SERVER, VIA};
use hyper::{Body, Client, Request, Response, Version};
use log::{debug, trace, warn};

/// A handler for proxy requests.
pub(super) struct ProxyHandler {
    /// The host from which the original request came
    remote_host: SocketAddr,

    /// The host:port to send the http request to.
    target_host: SocketAddr,

    client: Client<HttpConnector>,
}

impl ProxyHandler {
    /// create new ProxyHandler from the supplied remote and target addresses.
    pub(super) fn new(remote_host: SocketAddr, target_host: SocketAddr) -> Self {
        // TODO: Is it a problem to create new client per request?
        let client = Client::new();
        ProxyHandler {
            remote_host,
            target_host,
            client,
        }
    }

    pub(super) fn serve(
        &self,
        mut req: Request<Body>,
    ) -> impl Future<Item = Response<Body>, Error = Error> {
        let query_part = if let Some(query) = req.uri().query() {
            format!("?{}", query)
        } else {
            "".to_string()
        };
        let upstream_uri = format!(
            "http://{}{}{}",
            &self.target_host,
            req.uri().path(),
            &query_part
        );
        debug!("handling request for: {}", &upstream_uri);
        trace!("handling request: {:#?}", &req);
        // TODO: should we handle parse errors? it will make the code much less
        // nicer for very rare occasion.
        *req.uri_mut() = upstream_uri.parse().unwrap();
        let headers = req.headers_mut();
        headers.append(
            HeaderName::from_static("x-forwarded-for"),
            self.remote_host.ip().to_string().parse().unwrap(),
        );
        headers.append(
            HeaderName::from_static("x-forwarded-port"),
            self.remote_host.port().to_string().parse().unwrap(),
        );

        self.client.request(req).then(move |result| {
            trace!(
                "response from upstream (for request: {}): {:#?}",
                &upstream_uri,
                &result
            );
            let our_response = match result {
                Ok(mut response) => {
                    let version = match response.version() {
                        Version::HTTP_09 => "0.9",
                        Version::HTTP_10 => "1.0",
                        Version::HTTP_11 => "1.1",
                        Version::HTTP_2 => "2.0",
                    };
                    let headers = response.headers_mut();

                    headers.append(VIA, format!("{} duwop-proxy", version).parse().unwrap());

                    // Append a "Server" header if not already present.
                    if !headers.contains_key(SERVER) {
                        headers.insert(SERVER, "duwop".parse().unwrap());
                    }
                    response
                }
                Err(err) => {
                    warn!("upstream returned error: {}", err);
                    return Either::B(displayed_error(
                        StatusCode::BAD_GATEWAY,
                        "Something went wrong, please try again later".into(),
                    ));
                }
            };
            Either::A(futures::future::ok(our_response))
        })
    }
}
