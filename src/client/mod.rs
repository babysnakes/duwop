use super::management::client::Client as MgmtClient;
use super::management::{Request, Response};

use std::io::{self, Write};
use std::path::PathBuf;

use failure::{format_err, Error, ResultExt};
use log::info;
use text_io::{try_read, try_scan};
use url::Url;

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

pub fn create_proxy_file(proxy_file: PathBuf, url: Option<Url>) -> Result<(), Error> {
    let url = match url {
        Some(url) => url,
        None => {
            print!("Please enter URL to reverse proxy (e.g. http://localhost:3000/):\n> ");
            let _ = io::stdout().flush(); // not interested in the result
            let s: String = try_read!()?;
            Url::parse(&s).context(format!("could not parse url from: {}", &s))?
        }
    };
    std::fs::write(&proxy_file, format!("proxy:{}", url.as_str()))
        .context(format!("writing url to {:?}", &proxy_file))
        .map_err(Error::from)?;
    info!("saved proxy file: {:?}", &proxy_file);
    Ok(())
}
