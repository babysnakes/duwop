mod errors;
mod reverse_proxy;
mod static_files;

use super::app_defaults::{LAUNCHD_SOCKET, LAUNCHD_TLS_SOCKET};
use super::state::{AppState, ServiceType};
use errors::*;

use std::fs::File;
use std::io::BufReader;
use std::net::{Ipv4Addr, SocketAddr};
use std::path::PathBuf;
use std::sync::{Arc, RwLock};

use failure::{format_err, Error, ResultExt};
use futures::{Future, Poll};
use http::StatusCode;
use hyper::header::HOST;
use hyper::{self, Body, Request, Response};
use log::{debug, error, info, trace};
use tokio::net::TcpListener;
use tokio_rustls::{
    rustls::{
        internal::pemfile::{certs, pkcs8_private_keys,rsa_private_keys},
        NoClientAuth, ServerConfig,
    },
    TlsAcceptor,
};

type ErrorFut = Box<Future<Item = Response<Body>, Error = Error> + Send>;

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

pub enum Server {
    Http {
        listener: ServerListener,
        state: Arc<RwLock<AppState>>,
    },
    Https {
        listener: ServerListener,
        state: Arc<RwLock<AppState>>,
        tls_acceptor: TlsAcceptor,
    },
}

pub enum ServerListener {
    TcpListener(SocketAddr),
    Launchd(String),
}

impl Server {
    pub fn new_http(port: u16, launchd: bool, state: Arc<RwLock<AppState>>) -> Result<Self, Error> {
        let listener = if launchd {
            ServerListener::Launchd(LAUNCHD_SOCKET.to_string())
        } else {
            ServerListener::TcpListener((Ipv4Addr::LOCALHOST, port).into())
        };
        Ok(Server::Http { listener, state })
    }

    pub fn new_https(
        port: u16,
        launchd: bool,
        cert: PathBuf,
        priv_key: PathBuf,
        state: Arc<RwLock<AppState>>,
    ) -> Result<Self, Error> {
        let tls_acceptor = get_tls_acceptor(cert, priv_key)?;
        let listener = if launchd {
            ServerListener::Launchd(LAUNCHD_TLS_SOCKET.to_string())
        } else {
            ServerListener::TcpListener((Ipv4Addr::LOCALHOST, port).into())
        };
        Ok(Server::Https {
            listener,
            state,
            tls_acceptor,
        })
    }

    /// Run the server
    pub fn run(self) -> Box<Future<Item = (), Error = ()> + Send> {
        // We can not launch hyper the default way because we might not have a
        // socket to bind to (e.g. in case we use launchd).
        use futures::stream::Stream;
        use hyper::server::conn::Http;

        match self {
            Server::Http { listener, state } => {
                let listener = listener.to_listener().unwrap();
                let http = Http::new();
                let state = Arc::clone(&state);
                Box::new(listener.incoming().map_err(|e| error!("{:?}", e)).for_each(
                    move |socket| {
                        let source_address = socket.peer_addr().unwrap();
                        let service = MainService {
                            state: Arc::clone(&state),
                            remote_addr: source_address,
                        };
                        tokio::spawn(
                            http.serve_connection(socket, service)
                                .map_err(|e| error!("{:?}", e)),
                        )
                    },
                ))
            }
            Server::Https {
                listener,
                state,
                tls_acceptor,
            } => {
                let listener = listener.to_listener().unwrap();
                let state = Arc::clone(&state);
                let acceptor = tls_acceptor.clone();
                let done =
                    listener
                        .incoming()
                        .map_err(|e| error!("{:?}", e))
                        .for_each(move |stream| {
                            let addr = stream.peer_addr().unwrap();
                            let state_clone = Arc::clone(&state);
                            let http = Http::new();
                            let done = acceptor
                                .accept(stream)
                                .map_err(|e| error!("{:?}", e))
                                .and_then(move |stream| {
                                    let service = MainService {
                                        state: state_clone,
                                        remote_addr: addr,
                                    };
                                    let done = http
                                        .serve_connection(stream, service)
                                        .map_err(|e| error!("{:?}", e));
                                    tokio::spawn(done)
                                });
                            tokio::spawn(done)
                        });
                Box::new(done)
            }
        }
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

/// Construct TLSAcceptor from the provided certs and private key.
fn get_tls_acceptor(cert: PathBuf, priv_key: PathBuf) -> Result<TlsAcceptor, Error> {
    macro_rules! get_reader {
        ($f:expr) => {{
            let f = File::open(&$f).context(format!("reading key/cert from {:?}", &$f))?;
            BufReader::new(f)
        }};
    }
    debug!("opening cert file ({:?}) and private key ({:?})", &cert, &priv_key);
    let mut cert_buffer = get_reader!(&cert);
    let certs = certs(&mut cert_buffer)
        .map_err(|_| format_err!("Failed to open certificate"))?;
    if certs.is_empty() { return Err(format_err!("invalid certificate {:?}", &cert))}
    trace!("certificates: {:?}", certs);
    let mut rsa_key_buffer = get_reader!(&priv_key);
    let rsa_keys = rsa_private_keys(&mut rsa_key_buffer)
        .map_err(|_| format_err!("Failed to read private key"))?;
    let mut pkcs8_key_buffer = get_reader!(&priv_key);
    let pkcs8_keys = pkcs8_private_keys(&mut pkcs8_key_buffer)
        .map_err(|_| format_err!("Failed to read private key"))?;
    // prefer pkcs keys
    let key = if !pkcs8_keys.is_empty() {
        pkcs8_keys[0].clone()
    } else if !rsa_keys.is_empty() {
        rsa_keys[0].clone()
    } else {
        return Err(format_err!("no valid keys found in {:?}", &priv_key))
    };
    trace!("key: {:?}", key);
    let mut config = ServerConfig::new(NoClientAuth::new());
    config
        .set_single_cert(certs, key)
        .context("importing certificate")?;
    Ok(TlsAcceptor::from(Arc::new(config)))
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
