mod errors;
mod reverse_proxy;
mod static_files;

use super::ssl;
use super::state::{AppState, ServiceType};
use errors::*;

use std::net::SocketAddr;
use std::path::PathBuf;
use std::sync::{Arc, RwLock};

use failure::{format_err, Error, ResultExt};
use futures::{stream::Stream, sync::mpsc, Future, Poll};
use http::StatusCode;
use hyper::header::HOST;
use hyper::server::conn::Http;
use hyper::{self, Body, Request, Response};
use log::{debug, error, info, trace};
use openssl::pkey::{PKey, Private};
use openssl::ssl::{SslAcceptor, SslMethod};
use openssl::x509::X509;
use tokio::net::TcpListener;
use tokio_openssl::SslAcceptorExt;

type ErrorFut = Box<dyn Future<Item = Response<Body>, Error = Error> + Send>;

enum MainFuture {
    Static(ErrorFut),
    ReverseProxy(ErrorFut),
    ErrorResponse(ErrorFut),
}

/// Hyper `Service` implementation that serves all requests.
struct MainService {
    state: Arc<RwLock<AppState>>,
    remote_addr: SocketAddr,
}

/// Serves http requests
pub struct HttpServer {
    state: Arc<RwLock<AppState>>,
    listener: ServerListener,
}

/// Serves https requests
pub struct HttpsServer {
    state: Arc<RwLock<AppState>>,
    listener: ServerListener,
    ca_cert: X509,
    ca_privkey: PKey<Private>,
}

pub enum ServerListener {
    TcpListener(SocketAddr),
    Launchd(String),
}

impl HttpServer {
    pub fn new(listener: ServerListener, state: Arc<RwLock<AppState>>) -> Result<Self, Error> {
        Ok(HttpServer { listener, state })
    }

    pub fn run(self) -> Box<dyn Future<Item = (), Error = ()> + Send> {
        let listener = self.listener.to_listener().unwrap();
        let http = Http::new();
        let state = Arc::clone(&self.state);
        Box::new(
            listener
                .incoming()
                .map_err(|e| error!("HTTP server: {:?}", e))
                .for_each(move |socket| {
                    let source_address = socket.peer_addr().unwrap();
                    let service = MainService {
                        state: Arc::clone(&state),
                        remote_addr: source_address,
                    };
                    tokio::spawn(
                        http.serve_connection(socket, service)
                            .map_err(|e| error!("HTTP server: {:?}", e)),
                    )
                }),
        )
    }
}

impl HttpsServer {
    pub fn new(
        listener: ServerListener,
        state: Arc<RwLock<AppState>>,
        cert: PathBuf,
        priv_key: PathBuf,
    ) -> Result<Self, Error> {
        let (ca_cert, ca_privkey) =
            ssl::load_ca_cert(priv_key, cert).context("loading CA certificate")?;
        Ok(HttpsServer {
            state,
            listener,
            ca_cert,
            ca_privkey,
        })
    }

    /// Run the HTTPS server. The provided 'rx' signals re-generating of the SSL
    /// certificate - this will update the certificate to include new SANs.
    pub fn run(self, rx: mpsc::Receiver<()>) -> Box<dyn Future<Item = (), Error = ()> + Send> {
        let listener = self.listener.to_listener().unwrap();
        let state = Arc::clone(&self.state);
        let acceptor_rc = Arc::new(RwLock::new(self.get_ssl_acceptor().unwrap()));
        let acceptor_clone = Arc::clone(&acceptor_rc);
        let replacer = rx.for_each(move |_| {
            info!("received certificate renew request");
            let locked = Arc::clone(&acceptor_clone);
            let mut unlocked = locked.write().unwrap();
            *unlocked = self.get_ssl_acceptor().unwrap();
            info!("successfully replaced certificate");
            Ok(())
        });
        let done = listener
            .incoming()
            .map_err(|e| error!("HTTPS server: {:?}", e))
            .for_each(move |stream| {
                let acceptor = {
                    let unlocked = acceptor_rc.read().unwrap();
                    unlocked.clone()
                };
                let addr = stream.peer_addr().unwrap();
                let state_clone = Arc::clone(&state);
                let http = Http::new();
                let done = acceptor
                    .accept_async(stream)
                    .map_err(|e| error!("HTTPS server: {:?}", e))
                    .and_then(move |stream| {
                        let service = MainService {
                            state: state_clone,
                            remote_addr: addr,
                        };
                        let done = http
                            .serve_connection(stream, service)
                            .map_err(|e| error!("HTTPS server: {:?}", e));
                        tokio::spawn(done)
                    });
                tokio::spawn(done)
            });
        Box::new(replacer.join(done).map(|(_, _)| ()))
    }

    fn get_ssl_acceptor(&self) -> Result<SslAcceptor, Error> {
        debug!("creating work certificate");
        let names = {
            let unlocked = self.state.read().unwrap();
            unlocked
                .services
                .keys()
                .map(|x| x.to_owned())
                .collect::<Vec<String>>()
        };
        let (tmp_cert, tmp_privkey) =
            ssl::mk_ca_signed_cert(&self.ca_cert, &self.ca_privkey, names)
                .context("creating work certificate")?;
        let mut acceptor = SslAcceptor::mozilla_intermediate(SslMethod::tls())?;
        acceptor.set_private_key(&tmp_privkey)?;
        acceptor.set_certificate(&tmp_cert)?;
        acceptor.check_private_key()?;
        debug!("certificate valid :)");
        Ok(acceptor.build())
    }
}

impl ServerListener {
    fn to_listener(&self) -> Result<TcpListener, Error> {
        match self {
            ServerListener::Launchd(name) => {
                info!("listening for web requests on launchd socket: {}", name);
                get_activation_socket(name)
            }
            ServerListener::TcpListener(addr) => {
                info!("listening for web requests on {}", &addr);
                TcpListener::bind(&addr)
                    .context("binding to requested por")
                    .map_err(Error::from)
            }
        }
    }
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
        debug!(
            "received request for key: {}, uri: {}, from: {}",
            key,
            &req.uri(),
            &self.remote_addr
        );
        match state.services.get(key) {
            None => MainFuture::ErrorResponse(Box::new(handle_404())),
            Some(service) => match service {
                ServiceType::StaticFiles(path) => {
                    MainFuture::Static(Box::new(static_files::serve(req, &PathBuf::from(path))))
                }
                ServiceType::ReverseProxy(addr) => {
                    let handler = reverse_proxy::ProxyHandler::new(self.remote_addr, *addr);
                    MainFuture::ReverseProxy(Box::new(handler.serve(req)))
                }
                ServiceType::InvalidConfig(message) => MainFuture::ErrorResponse(Box::new(
                    displayed_error(StatusCode::INTERNAL_SERVER_ERROR, message.to_string()),
                )),
            },
        }
    }
}

fn extract_host<'a>(req: &'a Request<Body>) -> Result<&'a str, Error> {
    trace!("extracting host from headers: {:#?}", req.headers());
    let host_value = match req.headers().get(HOST) {
        Some(host) => host,
        None => return Err(format_err!("Couldn't extract header host")),
    };
    let host_header = host_value.to_str()?;
    let host = host_header.split(':').nth(0).unwrap_or("");
    if host.ends_with(".test") {
        Ok(&host[..host.len() - 5])
    } else {
        Err(format_err!("invalid host domain: {}", &host))
    }
}

#[cfg(target_os = "macos")]
use libc::size_t;
#[cfg(target_os = "macos")]
use std::os::raw::{c_char, c_int, c_void};

#[cfg(target_os = "macos")]
extern "C" {
    fn launch_activate_socket(name: *const c_char, fds: *mut *mut c_int, cnt: *mut size_t)
        -> c_int;
}

#[cfg(target_os = "macos")]
fn get_activation_socket(socket_name: &str) -> Result<TcpListener, Error> {
    use libc::free;
    use std::ffi::CString;
    use std::os::unix::io::FromRawFd;
    use std::ptr::null_mut;
    use tokio::reactor::Handle;
    unsafe {
        let mut fds: *mut c_int = null_mut();
        let mut cnt: size_t = 0;

        let name = CString::new(socket_name).expect("CString::new failed");
        match launch_activate_socket(name.as_ptr(), &mut fds, &mut cnt) {
            0 => {
                debug!("name is {:?}, cnt is {}, fds is {:#?}", name, cnt, fds);
                if cnt == 1 {
                    let std_listener = std::net::TcpListener::from_raw_fd(*fds.offset(0));
                    free(fds as *mut c_void);
                    let listener = TcpListener::from_std(std_listener, &Handle::default())?;
                    Ok(listener)
                } else {
                    Err(format_err!(
                        "Could not get fd: cnt should be 1 but is {}",
                        cnt
                    ))
                }
            }
            n => Err(format_err!(
                "Could not get fd: launch_activate_socket != 0 (received: {})",
                n
            )),
        }
    }
}

// While it's not currently intended to be supported on other platforms, one
// might want to use specific functionality on different platform so we might as
// well make it compile.
#[cfg(not(target_os = "macos"))]
fn get_activation_socket(_: &str) -> Result<TcpListener, Error> {
    Err(format_err!("Only supported on macos"))
}

#[cfg(test)]
pub mod tests {
    use super::*;

    use std::net::Ipv4Addr;

    use std::collections::HashMap;
    use std::sync::{Arc, RwLock};

    use hyper::{service::Service, Request, StatusCode};

    pub fn test_web_service(
        req: Request<Body>,
        state: Arc<RwLock<AppState>>,
    ) -> Result<Response<Body>, Error> {
        let mut service = MainService {
            state,
            remote_addr: (Ipv4Addr::LOCALHOST, 10000).into(), // demo remote address
        };
        let response = service.call(req);
        let mut runtime = tokio::runtime::Runtime::new().expect("Unable to create a runtime");
        runtime.block_on(response)
    }

    pub fn construct_state(kv: Vec<(String, ServiceType)>) -> Arc<RwLock<AppState>> {
        let mut map = HashMap::new();
        for (k, v) in kv {
            map.insert(k, v);
        }
        let state = AppState::from_services(map);
        Arc::new(RwLock::new(state))
    }

    #[test]
    fn extract_host_extracts_the_host_without_domain() {
        let request = Request::builder()
            .header("host", "example.test")
            .body(Body::empty())
            .unwrap();
        let result = extract_host(&request).unwrap();
        assert_eq!(result, "example");
    }

    #[test]
    fn extract_host_returns_error_if_no_host_header_is_found() {
        let request = Request::builder().body(Body::empty()).unwrap();
        let result = extract_host(&request);
        assert!(result.is_err(), "no host should return error");
    }

    #[test]
    fn extract_host_returns_error_if_domain_is_not_valid() {
        let request = Request::builder()
            .header("host", "example.com")
            .body(Body::empty())
            .unwrap();
        let response = extract_host(&request);
        assert!(response.is_err(), "invalid domain should return error");
    }

    #[test]
    fn respond_correctly_to_static_file_request() {
        let state = construct_state(vec![(
            "project-dir".to_string(),
            ServiceType::StaticFiles(std::env::current_dir().unwrap()),
        )]);
        let request = Request::builder()
            .header("host", "project-dir.test")
            .uri("http://project-dir.test/Cargo.toml")
            .body(Body::empty())
            .unwrap();
        let response = test_web_service(request, state).unwrap();
        assert_eq!(response.status(), StatusCode::OK)
    }

    #[test]
    fn request_without_host_should_return_500_error() {
        let state = construct_state(vec![(
            "key".to_string(),
            ServiceType::StaticFiles("/some/path".into()),
        )]);
        let request = Request::builder().body(Body::empty()).unwrap();
        let response = test_web_service(request, state).unwrap();
        assert_eq!(response.status(), StatusCode::INTERNAL_SERVER_ERROR);
    }

    #[test]
    fn request_with_invalid_domain_should_return_500() {
        let state = construct_state(vec![(
            "key".to_string(),
            ServiceType::StaticFiles("/some/path".into()),
        )]);
        let request = Request::builder()
            .header("host", "example.com")
            .body(Body::empty())
            .unwrap();
        let response = test_web_service(request, state).unwrap();
        assert_eq!(response.status(), StatusCode::INTERNAL_SERVER_ERROR);
    }

    #[test]
    fn request_with_undefined_host_should_return_404() {
        let state = construct_state(vec![(
            "key".to_string(),
            ServiceType::StaticFiles("/some/path".into()),
        )]);
        let request = Request::builder()
            .header("host", "undefined.test")
            .body(Body::empty())
            .unwrap();
        let response = test_web_service(request, state).unwrap();
        assert_eq!(response.status(), StatusCode::NOT_FOUND);
    }

    #[test]
    fn request_for_invalid_config_should_return_messaged_error() {
        let state = construct_state(vec![(
            "key".to_string(),
            ServiceType::InvalidConfig("invalid config".to_string()),
        )]);
        let request = Request::builder()
            .header("host", "key.test")
            .body(Body::empty())
            .unwrap();
        let response = test_web_service(request, state).unwrap();
        assert_eq!(response.status(), StatusCode::INTERNAL_SERVER_ERROR);
        // TODO: validating the body is more involved and currently not worth
        // it.
    }
}
