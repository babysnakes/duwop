use super::{Request, Response};

use std::io::{BufRead, BufReader, Write};
use std::net::{Ipv4Addr, TcpStream};

use failure::{format_err, Error, ResultExt};
use log::debug;

/// A management client, always connects to localhost!
pub struct Client {
    port: u16,
}

impl Client {
    /// constructs a new client to to the provided port
    pub fn new(port: u16) -> Self {
        Client { port }
    }

    /// Runs a single command (request) to the server and expects a single response.
    pub fn run_client_command(&self, req: Request) -> Result<Response, Error> {
        let mut stream =
            TcpStream::connect((Ipv4Addr::LOCALHOST, self.port)).context("Connecting to server")?;
        let mut msg = req.serialize();
        msg.push('\n');
        stream
            .write(msg.as_bytes())
            .context("sending command to the server")?;
        debug!("Send command to the server ({})", req.serialize());
        let mut response = String::new();
        let mut reader = BufReader::new(stream);
        reader
            .read_line(&mut response)
            .context("reading response")?;
        Response::parse(&response).map_err(|e| format_err!("error response from server: {}", e))
    }
}
