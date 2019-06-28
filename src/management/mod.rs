pub mod client;

use std::io::BufReader;
use std::net::{Ipv4Addr, SocketAddr};
use std::sync::{Arc, RwLock};

use super::state::AppState;

use failure::{format_err, Error};
use flexi_logger::{LogSpecification, ReconfigurationHandle};
use log::info;
use tokio;
use tokio::io::{lines, write_all};
use tokio::net::TcpListener;
use tokio::prelude::*;

#[derive(Clone)]
pub struct Server {
    port: u16,
    state: Arc<RwLock<AppState>>,
    log_handler: Arc<RwLock<ReconfigurationHandle>>,
    log_level: String,
}

/// Protocol request
#[derive(Debug, PartialEq)]
pub enum Request {
    /// Reload the state from disk.
    ReloadState,

    /// Set log level
    SetLogLevel(LogLevel),

    /// Reset log level
    ResetLogLevel,

    /// Query server for it's status, currently only indicates that it is running
    ServerStatus,
}

/// Represents various log level commands.
#[derive(Debug, PartialEq)]
pub enum LogLevel {
    /// Set debug log level
    DebugLevel,

    /// Set trace log level
    TraceLevel,

    /// Set custom Level (similar to setting RUST_LOG)
    CustomLevel(String),
}

/// Protocol response
pub enum Response {
    // A response without specific text
    Done,
    // Error message
    Error(String),
}

impl Server {
    /// New management server running on provided port and holding the provided
    /// app state and log handler. *log_level* is the default application log
    /// level (to use when resetting log level).
    pub fn new(
        port: u16,
        state: Arc<RwLock<AppState>>,
        log_handler: Arc<RwLock<ReconfigurationHandle>>,
        log_level: String,
    ) -> Self {
        Server { port, state, log_handler, log_level }
    }

    /// Returns a future to run the management server.
    pub fn run(self) -> Box<impl Future<Item = (), Error = ()> + Send> {
        let addr: SocketAddr = (Ipv4Addr::LOCALHOST, self.port).into();
        info!("listening for management requests on {}", &addr);
        let listener = TcpListener::bind(&addr).unwrap();
        Box::new(
            listener
                .incoming()
                .map_err(|e| println!("error accepting socket; error = {:?}", e))
                .for_each(move |socket| {
                    let (reader, writer) = socket.split();
                    let lines = lines(BufReader::new(reader));
                    let mut server = self.clone();
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
                            Request::SetLogLevel(log_level) => {
                                match server.set_log_level(log_level) {
                                    Ok(()) => Response::Done,
                                    Err(e) => {
                                        Response::Error(format!("error setting log level: {}", e))
                                    }
                                }
                            }
                            Request::ResetLogLevel => match server.reset_log_level() {
                                Ok(()) => Response::Done,
                                Err(e) => {
                                    Response::Error(format!("error setting log level: {}", e))
                                }
                            },
                            Request::ServerStatus => match server.status() {
                                Ok(_) => Response::Done,
                                Err(e) => Response::Error(format!(
                                    "error querying server for status: {}",
                                    e
                                )),
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

    fn set_log_level(&mut self, level: LogLevel) -> Result<(), Error> {
        info!("setting log level to: {:?}", level);
        let locked = Arc::clone(&self.log_handler);
        let mut handler = locked.write().unwrap();
        let spec = match level {
            LogLevel::DebugLevel => LogSpecification::parse("duwop=debug")?,
            LogLevel::TraceLevel => LogSpecification::parse("duwop=trace")?,
            LogLevel::CustomLevel(value) => LogSpecification::parse(&value)?,
        };
        handler.set_new_spec(spec);
        Ok(())
    }

    fn reset_log_level(&mut self) -> Result<(), Error> {
        info!("resetting log level to: {}", &self.log_level);
        let locked = Arc::clone(&self.log_handler);
        let mut handler = locked.write().unwrap();
        let spec = LogSpecification::parse(&self.log_level)?;
        handler.set_new_spec(spec);
        Ok(())
    }

    fn status(&self) -> Result<(), Error> {
        info!("received status request");
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
            Some("Log") => match parts.next() {
                Some("reset") => Ok(Request::ResetLogLevel),
                Some("debug") => Ok(Request::SetLogLevel(LogLevel::DebugLevel)),
                Some("trace") => Ok(Request::SetLogLevel(LogLevel::TraceLevel)),
                Some("custom") => match parts.next() {
                    // TODO: should we validate input? I managed to mess with the logger :(
                    Some(value) => Ok(Request::SetLogLevel(LogLevel::CustomLevel(
                        value.to_string(),
                    ))),
                    None => Err("custom log level requires value".to_string()),
                },
                Some(cmd) => Err(format!("invalid log command: {}", cmd)),
                None => Err("Log requires command".to_string()),
            },
            Some("Status") => Ok(Request::ServerStatus),
            Some(cmd) => Err(format!("invalid command: {}", cmd)),
            None => Err("empty input".to_string()),
        }
    }

    fn serialize(&self) -> String {
        match self {
            Request::ReloadState => "Reload".to_string(),
            Request::SetLogLevel(level) => match level {
                LogLevel::DebugLevel => "Log debug".to_string(),
                LogLevel::TraceLevel => "Log trace".to_string(),
                LogLevel::CustomLevel(value) => format!("Log custom {}", value),
            },
            Request::ResetLogLevel => "Log reset".to_string(),
            Request::ServerStatus => "Status".to_string(),
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

#[cfg(test)]
mod tests {
    use super::*;

    macro_rules! request_parse_ok {
        ($name:ident, $input:expr, $expected:expr) => {
            #[test]
            fn $name() {
                let result = Request::parse($input).unwrap();
                assert_eq!(result, $expected);
            }
        };
    }

    macro_rules! request_parse_err {
        ($name:ident, $input:expr, $expected:expr) => {
            #[test]
            fn $name() {
                let result = Request::parse($input);
                assert!(result.is_err());
                let msg = result.unwrap_err();
                assert!(msg.contains($expected))
            }
        };
    }

    macro_rules! request_serialize {
        ($name:ident, $request:expr, $expected:expr) => {
            #[test]
            fn $name() {
                assert_eq!($request.serialize(), $expected.to_string());
            }
        };
    }

    request_parse_ok! { parse_reload, "Reload", Request::ReloadState }
    request_parse_ok! { parse_reset_log, "Log reset", Request::ResetLogLevel }
    request_parse_ok! {
        parse_debug_log_level, "Log debug", Request::SetLogLevel(LogLevel::DebugLevel)
    }
    request_parse_ok! {
        parse_trace_log_level, "Log trace", Request::SetLogLevel(LogLevel::TraceLevel)
    }
    request_parse_ok! {
        parse_custom_log_level, "Log custom debug, duwop=trace",
        Request::SetLogLevel(LogLevel::CustomLevel("debug, duwop=trace".to_string()))
    }

    request_parse_ok! { parse_service_status, "Status", Request::ServerStatus }

    request_parse_err! { parse_reload_with_with_argument, "Reload more", "arguments" }
    request_parse_err! { parse_invalid_log_level_command, "Log invalid", "invalid log command" }
    request_parse_err! { parse_log_without_command, "Log", "Log requires command" }
    request_parse_err! { parse_log_custom_without_value, "Log custom", "custom log level requires value"}
    request_parse_err! { parse_invalid_input, "UNDEFINED", "invalid" }
    request_parse_err! { parse_empty_request, "", "invalid" }

    request_serialize! { serialize_reload_state, Request::ReloadState, "Reload" }
    request_serialize! { serialize_reset_log, Request::ResetLogLevel, "Log reset" }
    request_serialize! {
        serialize_debug_log, Request::SetLogLevel(LogLevel::DebugLevel), "Log debug"
    }
    request_serialize! {
        serialize_trace_log, Request::SetLogLevel(LogLevel::TraceLevel), "Log trace"
    }
    request_serialize! {
        serialize_custom_log,
        Request::SetLogLevel(LogLevel::CustomLevel("info, duwop:trace".to_string())),
        "Log custom info, duwop:trace"
    }
    request_serialize! { serialize_server_status, Request::ServerStatus, "Status" }
}
