use super::management::client::Client as MgmtClient;
use super::management::{Request, Response};

use std::path::PathBuf;

use failure::{format_err, Error, ResultExt};
use log::info;

fn process_client_response(result: Result<Response, Error>) -> Result<(), Error> {
    match result {
        Ok(resp) => {
            let msg = resp.serialize();
            match resp {
                Response::Done => {
                    info!("{}", msg);
                    Ok(())
                }
                Response::Error(_) => Err(format_err!("Error from server: {}", msg)),
            }
        }
        Err(err) => Err(err),
    }
}

pub fn run_reload(port: u16) -> Result<(), Error> {
    let client = MgmtClient::new(port);
    process_client_response(client.run_client_command(Request::ReloadState))
}

pub fn run_log(port: u16, request: Request) -> Result<(), Error> {
    let client = MgmtClient::new(port);
    process_client_response(client.run_client_command(request))
}

pub fn link_web_directory(state_dir: PathBuf, web_dir: PathBuf) -> Result<(), Error> {
    let dir = std::fs::canonicalize(web_dir).context("normalizing web directory path")?;
    std::os::unix::fs::symlink(dir, state_dir)
        .context("Linking web directory")
        .map_err(Error::from)
}
