pub mod client;

use std::io::BufReader;
use std::net::{Ipv4Addr, SocketAddr};
use std::sync::{Arc, RwLock};

use super::state::AppState;

use failure::{format_err, Error};
use log::info;
use tokio;
use tokio::io::{lines, write_all};
use tokio::net::TcpListener;
use tokio::prelude::*;

#[derive(Clone)]
pub struct Server {
    state: Arc<RwLock<AppState>>,
}

/// Protocol request
pub enum Request {
    /// Reload the state from disk.
    ReloadState,
}

/// Protocol response
pub enum Response {
    // A response without specific text
    Done,
    // Error message
    Error(String),
}

impl Server {
    /// New management server with provided mutable state
    pub fn run(
        port: u16,
        state: Arc<RwLock<AppState>>,
    ) -> Box<impl Future<Item = (), Error = ()> + Send> {
        let server = Server { state };
        let addr: SocketAddr = (Ipv4Addr::LOCALHOST, port).into();
        info!("listening for management requests on {}", &addr);
        let listener = TcpListener::bind(&addr).unwrap();
        Box::new(
            listener
                .incoming()
                .map_err(|e| println!("error accepting socket; error = {:?}", e))
                .for_each(move |socket| {
                    let (reader, writer) = socket.split();
                    let lines = lines(BufReader::new(reader));
                    let mut server = server.clone();
                    let responses = lines.map(move |line| {
                        let request = match Request::parse(&line) {
                            Ok(req) => req,
                            Err(e) => return Response::Error(e),
                        };
                        match request {
                            Request::ReloadState => match server.reload_state() {
                                Ok(()) => Response::Done,
                                Err(e) => Response::Error(format!("error reloading: {}", e)),
                            },
                        }
                    });
                    let writes = responses.fold(writer, |writer, response| {
                        let mut response = response.serialize();
                        response.push('\n');
                        write_all(writer, response.into_bytes()).map(|(w, _)| w)
                    });
                    // TODO: This is copied from tokio tinydb example. It ignores
                    // errors. Should we do something else here?
                    let msg = writes.then(move |_| Ok(()));
                    tokio::spawn(msg)
                }),
        )
    }

    fn reload_state(&mut self) -> Result<(), Error> {
        let state = Arc::clone(&self.state);
        let mut unlocked = state.write().unwrap();
        unlocked.load_services()?;
        Ok(())
    }
}

impl Request {
    fn parse(input: &str) -> Result<Request, String> {
        let mut parts = input.splitn(3, ' ');
        match parts.next() {
            Some("Reload") => {
                if parts.next().is_some() {
                    return Err("Reload doesn't take arguments".to_string());
                };
                Ok(Request::ReloadState)
            }
            Some(cmd) => Err(format!("invalid command: {}", cmd)),
            None => Err("empty input".to_string()),
        }
    }

    fn serialize(&self) -> String {
        match self {
            Request::ReloadState => "Reload".to_string(),
        }
    }
}

impl Response {
    fn parse(input: &str) -> Result<Response, Error> {
        let mut parts = input.splitn(2, ' ');
        match parts.next().map(|s| s.trim()) {
            Some("OK") => Ok(Response::Done),
            Some("ERROR") => {
                let error = parts.next().unwrap_or("");
                Ok(Response::Error(error.to_string()))
            }
            Some(_) => Err(format_err!("invalid response from server: {}", &input)),
            None => Err(format_err!("no response from server")),
        }
    }

    pub fn serialize(&self) -> String {
        let ok = "OK".to_string();
        let error = "ERROR".to_string();
        match self {
            Response::Done => ok,
            Response::Error(m) => format!("{} {}", error, m),
        }
    }
}
