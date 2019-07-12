use super::cli_helpers::LogCommand;
use super::management::client::Client as MgmtClient;
use super::management::{LogLevel, Request, Response};
use super::state::{AppState, ServiceConfigError, ServiceType};

use std::collections::HashMap;
use std::ffi::OsString;
use std::fmt;
use std::io::{self, Write};
use std::net::{IpAddr, Ipv4Addr};
use std::path::PathBuf;

use dns_lookup::lookup_host;
use failure::{format_err, Error, ResultExt};
use log::info;
use text_io::{try_read, try_scan};
use url::Url;
use yansi::Paint;

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
        }
        Ok(())
    }

    pub fn doctor(&self) -> Result<(), Error> {
        let mut status = Status::new();

        info!("querying server status");
        let client = MgmtClient::new(self.management_port);
        if let Err(err) = process_client_response(client.run_client_command(Request::ServerStatus))
        {
            status.server_status = Some(err);
        }

        info!("querying database status");
        let mut state = AppState::new(&self.state_dir);
        state.load_services()?;
        for (k, v) in &state.services {
            if let ServiceType::InvalidConfig(msg) = v {
                status.invalid_configurations.insert(k.clone(), msg.clone());
            }
        }
        for e in state.errors() {
            match e {
                ServiceConfigError::NameError(path) => {
                    status.name_errors.push(path.clone());
                }
                ServiceConfigError::IoError(msg) => {
                    status.io_errors.push(msg.clone());
                }
            }
        }

        info!("Querying DNS resolving");
        match lookup_host("abcd.test") {
            Ok(results) => {
                if results.contains(&IpAddr::V4(Ipv4Addr::LOCALHOST)) {
                    status.dns_resolving = None;
                } else {
                    let msg = format!(
                        "expected results to contain 127.0.0.1, found: {:?}",
                        results
                    );
                    status.dns_resolving = Some(msg);
                }
            }
            Err(err) => {
                let msg = format!("error resolving: {}", err);
                status.dns_resolving = Some(msg);
            }
        }

        println!("\n{}", status);
        Ok(())
    }
}

/// Holds the result of all status queries.
struct Status {
    /// Contains the errors from querying the service via management interface.
    server_status: Option<Error>,

    /// Containing the various invalid configurations (service key -> error).
    invalid_configurations: HashMap<String, String>,

    /// Messages from io errors while reading database - i.e. invalid links, etc.
    io_errors: Vec<String>,

    /// A list of paths that could not be converted to strings - non-unicode.
    name_errors: Vec<OsString>,

    /// String indicates DNS resolving error
    dns_resolving: Option<String>,
}

impl Status {
    fn new() -> Self {
        Status {
            server_status: None,
            invalid_configurations: HashMap::new(),
            io_errors: vec![],
            name_errors: vec![],
            dns_resolving: Some("Not Yet Tested".to_string()),
        }
    }

    /// Indicates that the database has no errors
    fn is_db_clean(&self) -> bool {
        self.invalid_configurations.is_empty()
            && self.io_errors.is_empty()
            && self.name_errors.is_empty()
    }
}

impl fmt::Display for Status {
    fn fmt(&self, f: &mut fmt::Formatter) -> fmt::Result {
        let error_arrow = format!("    {} ", Paint::red("->"));
        let wrapper = textwrap::Wrapper::with_termwidth()
            .initial_indent(&error_arrow)
            .subsequent_indent("       ");

        match &self.server_status {
            Some(err) => {
                writeln!(f, "Server Status: {}", Paint::red("Error"))?;
                writeln!(f, "{}", wrapper.fill(&err.to_string()))?;
                for cause in err.iter_causes() {
                    writeln!(f, "{}", wrapper.fill(&cause.to_string()))?;
                }
            }
            None => {
                writeln!(f, "Server Status: {}", Paint::green("Ok"))?;
            }
        }

        match &self.dns_resolving {
            Some(err) => {
                writeln!(f, "DNS Resolving: {}", Paint::red("Error"))?;
                writeln!(f, "{}", wrapper.fill(err))?;
            }
            None => {
                writeln!(f, "DNS Resolving: {}", Paint::green("Ok"))?;
            }
        }

        if self.is_db_clean() {
            writeln!(f, "Database Status: {}", Paint::green("OK"))?;
        } else {
            if !self.invalid_configurations.is_empty() {
                writeln!(f, "Database Status: {}", Paint::red("ERROR"))?;
                writeln!(f, "    The following services have configuration errors:")?;
                for (service, err) in &self.invalid_configurations {
                    let msg = format!("{}: {}", service, err);
                    writeln!(f, "{}", wrapper.fill(&msg))?;
                }
            }
            if !self.name_errors.is_empty() {
                writeln!(f, "    The following paths have non UTF-8 characters:")?;
                for path in &self.name_errors {
                    let p = format!("{:?}", path);
                    writeln!(f, "{}", wrapper.fill(&p))?;
                }
            }
            if !self.io_errors.is_empty() {
                writeln!(
                    f,
                    "    The following IO errors occurred while reading the database;"
                )?;
                for err in &self.io_errors {
                    writeln!(f, "{}", &err)?;
                }
            }
        }
        Ok(())
    }
}

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
