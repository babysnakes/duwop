use super::cli_helpers::LogCommand;
use super::management::client::Client as MgmtClient;
use super::management::{LogLevel, Request, Response};
use super::state::{AppState, ServiceType};

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

pub struct DuwopClient {
    management_port: u16,
    state_dir: PathBuf,
}

impl DuwopClient {
    pub fn new(management_port: u16, state_dir: PathBuf) -> Self {
        DuwopClient {
            management_port,
            state_dir,
        }
    }

    pub fn reload_server_configuration(&self) -> Result<(), Error> {
        let client = MgmtClient::new(self.management_port);
        process_client_response(client.run_client_command(Request::ReloadState))
    }

    pub fn run_log_command(
        &self,
        cmd: LogCommand,
        custom_level: Option<String>,
    ) -> Result<(), Error> {
        let request = match cmd {
            LogCommand::Debug => Request::SetLogLevel(LogLevel::DebugLevel),
            LogCommand::Trace => Request::SetLogLevel(LogLevel::TraceLevel),
            LogCommand::Custom => {
                let level = custom_level.unwrap(); // should be protected by clap 'requires'
                Request::SetLogLevel(LogLevel::CustomLevel(level))
            }
            LogCommand::Reset => Request::ResetLogLevel,
        };
        let client = MgmtClient::new(self.management_port);
        process_client_response(client.run_client_command(request))
    }

    pub fn create_static_file_configuration(
        &self,
        name: String,
        source_dir: Option<PathBuf>,
    ) -> Result<(), Error> {
        let source_dir = match source_dir {
            Some(path) => path,
            None => std::env::current_dir()
                .context("extracting working directory for source web directory")?,
        };
        let mut link = self.state_dir.clone();
        link.push(&name);
        let web_dir =
            std::fs::canonicalize(source_dir).context("normalizing web directory path")?;
        let st = ServiceType::StaticFiles(web_dir.clone());
        st.create(&link)?;
        info!(
            "created static file service '{}' pointing to {:?}",
            &name, &web_dir
        );
        self.reload_server_configuration()
    }

    pub fn create_proxy_configuration(&self, name: String, url: Option<Url>) -> Result<(), Error> {
        let mut proxy_file = self.state_dir.clone();
        proxy_file.push(name);
        let url = match url {
            Some(url) => url,
            None => {
                print!("Please enter URL to reverse proxy (e.g. http://localhost:3000/):\n> ");
                let _ = io::stdout().flush(); // not interested in the result
                let s: String = try_read!()?;
                Url::parse(&s).context(format!("could not parse url from: {}", &s))?
            }
        };
        let st = ServiceType::ReverseProxy(url);
        st.create(&proxy_file)?;
        info!("saved proxy file: {:?}", &proxy_file);
        self.reload_server_configuration()
    }

    pub fn delete_configuration(&self, name: String) -> Result<(), Error> {
        let mut to_delete = self.state_dir.clone();
        to_delete.push(&name);
        std::fs::remove_file(&to_delete).context(format!(
            "Deleting configuration '{}' (file: {:?}) failed! Are you sure it exists?",
            &name, &to_delete
        ))?;
        info!("successfully deleted service '{}'", &name);
        self.reload_server_configuration()
    }

    pub fn print_services(&self) -> Result<(), Error> {
        let mut state = AppState::new(&self.state_dir);
        state.load_services()?;
        let mut keys: Vec<&String> = state.services.keys().collect();
        keys.sort();
        for k in keys {
            let v = state.services.get(k).unwrap();
            v.pprint(&k);
        };
        Ok(())
    }
}
