use super::app_defaults::*;
use super::cli_helpers::LogCommand;
use super::management::client::Client as MgmtClient;
use super::management::{LogLevel, Request, Response};
use super::state::{AppState, ServiceConfigError, ServiceType};

use std::collections::HashMap;
use std::ffi::OsString;
use std::fmt;
use std::net::{IpAddr, Ipv4Addr, SocketAddr};
use std::path::PathBuf;

use dialoguer::Confirmation;
use dns_lookup::lookup_host;
use failure::{format_err, Error, ResultExt};
use log::info;
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
        self.handle_request(Request::ReloadState)
    }

    pub fn reload_ssl(&self) -> Result<(), Error> {
        self.handle_request(Request::ReloadSsl)
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
        name: Option<String>,
        source_dir: Option<PathBuf>,
    ) -> Result<(), Error> {
        self.parse_create_static_file_configuration_input(name, source_dir)
            .and_then(|(web_dir, name)| ServiceType::StaticFiles(web_dir.clone()).create(&name))
            .and_then(|_| {
                info!("run 'reload-ssl' if you want to access the new service with https");
                self.reload_server_configuration()
            })
    }

    pub fn parse_create_static_file_configuration_input(
        &self,
        name: Option<String>,
        source_dir: Option<PathBuf>,
    ) -> Result<(PathBuf, String), Error> {
        let source_dir = match source_dir {
            Some(path) => path,
            None => std::env::current_dir()
                .context("extracting working directory for source web directory")?,
        };
        let service_name = match name {
            Some(name) => name,
            None => source_dir
                .file_name()
                .ok_or_else(|| format_err!("couldn't extract service name from source dir"))?
                .to_str()
                .ok_or_else(|| format_err!("invalid utf8 directory name"))?
                .to_owned(),
        };
        let mut link = self.state_dir.clone();
        link.push(&service_name);
        let web_dir =
            std::fs::canonicalize(source_dir).context("normalizing web directory path")?;
        Ok((web_dir, link.to_str().unwrap().to_owned()))
    }

    pub fn create_proxy_configuration(&self, name: String, port: u16) -> Result<(), Error> {
        let mut proxy_file = self.state_dir.clone();
        proxy_file.push(name);
        let addr: SocketAddr = (Ipv4Addr::LOCALHOST, port).into();
        ServiceType::ReverseProxy(addr).create(&proxy_file)?;
        info!("run 'reload-ssl' if you want to access the new service with https");
        self.reload_server_configuration()
    }

    pub fn delete_configuration(&self, name: String, confirm: bool) -> Result<(), Error> {
        let message = format!("{} delete service {}?", Paint::green("??"), &name,);
        if !confirm
            && !Confirmation::new()
                .with_text(&message)
                .default(false)
                .interact()?
        {
            return Err(format_err!("cancelled"));
        };
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
        info!("querying server status");
        let server_status = self.check_server_status();
        info!("querying database status");
        let (invalid_configurations, name_errors, io_errors) = self.check_database_status()?;
        info!("Querying DNS resolving");
        let dns_resolving = check_lookup_host();
        info!("Querying CA status");
        let ca_valid = validate_ca(CA_KEY.to_owned(), CA_CERT.to_owned())?;

        let status = Status {
            server_status,
            invalid_configurations,
            name_errors,
            io_errors,
            dns_resolving,
            ca_valid,
        };
        println!("\n{}", status);
        Ok(())
    }

    fn check_server_status(&self) -> Result<(), Error> {
        self.handle_request(Request::ServerStatus)
    }

    fn check_database_status(
        &self,
    ) -> Result<(InvalidConfigurations, NameErrors, IoErrors), Error> {
        let mut state = AppState::new(&self.state_dir);
        state.load_services()?;
        let invalids: HashMap<String, String> = state
            .services
            .iter()
            .filter_map(|(k, v)| match v {
                ServiceType::InvalidConfig(msg) => Some((k.to_owned(), msg.to_owned())),
                _ => None,
            })
            .collect();
        let name_errors: Vec<OsString> = state
            .errors()
            .iter()
            .filter_map(|elem| match elem {
                ServiceConfigError::NameError(path) => Some(path.to_owned()),
                _ => None,
            })
            .collect();
        let io_errors: Vec<String> = state
            .errors()
            .iter()
            .filter_map(|elem| match elem {
                ServiceConfigError::IoError(msg) => Some(msg.to_owned()),
                _ => None,
            })
            .collect();
        Ok((invalids, name_errors, io_errors))
    }

    fn handle_request(&self, req: Request) -> Result<(), Error> {
        let client = MgmtClient::new(self.management_port);
        process_client_response(client.run_client_command(req))
    }
}

/// This function checks that the system resolves correctly. It doesn't check
/// the dns service. As such, it's only valid at "production".
fn check_lookup_host() -> Option<String> {
    match lookup_host("abcd.test") {
        Ok(results) => {
            if results.contains(&IpAddr::V4(Ipv4Addr::LOCALHOST)) {
                None
            } else {
                let msg = format!(
                    "expected results to contain 127.0.0.1, found: {:?}",
                    results
                );
                Some(msg)
            }
        }
        Err(err) => {
            let msg = format!("error resolving: {}", err);
            Some(msg)
        }
    }
}

fn validate_ca(key: PathBuf, cert: PathBuf) -> Result<Option<bool>, Error> {
    use super::ssl::*;

    if key.exists() && cert.exists() {
        let (cert, _key) = load_ca_cert(key, cert)?;
        if validate_ca(cert, CA_EXPIRED_GRACE)? {
            Ok(Some(true))
        } else {
            Ok(Some(false))
        }
    } else {
        Ok(None)
    }
}

type InvalidConfigurations = HashMap<String, String>;
type NameErrors = Vec<OsString>;
type IoErrors = Vec<String>;

/// Holds the result of all status queries.
struct Status {
    /// Contains the errors from querying the service via management interface.
    server_status: Result<(), Error>,

    /// Containing the various invalid configurations (service key -> error).
    invalid_configurations: InvalidConfigurations,

    /// Messages from io errors while reading database - i.e. invalid links, etc.
    io_errors: IoErrors,

    /// A list of paths that could not be converted to strings - non-unicode.
    name_errors: NameErrors,

    /// String indicates DNS resolving error
    dns_resolving: Option<String>,

    /// Indicates valid CA in terms of dates. None represents not configured.
    ca_valid: Option<bool>,
}

impl Status {
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
            Ok(()) => {
                writeln!(f, "Server Status: {}", Paint::green("Ok"))?;
            }
            Err(err) => {
                writeln!(f, "Server Status: {}", Paint::red("Error"))?;
                writeln!(f, "{}", wrapper.fill(&err.to_string()))?;
                for cause in err.iter_causes() {
                    writeln!(f, "{}", wrapper.fill(&cause.to_string()))?;
                }
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
            writeln!(f, "Database Status: {}", Paint::red("ERROR"))?;
            if !self.invalid_configurations.is_empty() {
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
                    writeln!(f, "{}", wrapper.fill(&err))?;
                }
            }
        }

        match self.ca_valid {
            None => {
                writeln!(
                    f,
                    "CA Valid: {}",
                    Paint::yellow("Ignored (probably not configured)")
                )?;
            }
            Some(valid) => {
                if valid {
                    writeln!(f, "CA Valid: {}", Paint::green("Ok"))?;
                } else {
                    writeln!(f, "CA Valid: {}", Paint::red("Error"))?;
                    writeln!(f, "{}", wrapper.fill("CA certificate expired or going to be expired soon. Run `duwop setup` to fix"))?;
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
            if let Response::Error(err) = resp {
                Err(format_err!("Error from server: {}", err))
            } else {
                info!("{}", msg);
                Ok(())
            }
        }
        Err(err) => Err(err),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    use std::path::PathBuf;

    fn parse_link_input(
        name: Option<String>,
        path: Option<PathBuf>,
    ) -> (DuwopClient, PathBuf, String) {
        let state_dir = assert_fs::TempDir::new().unwrap();
        let client = DuwopClient::new(1111, state_dir.path().to_path_buf());
        let (path, name) = client
            .parse_create_static_file_configuration_input(name, path)
            .unwrap();
        (client, path, name)
    }

    #[test]
    fn test_validate_ca_non_existing_cert() {
        let key = PathBuf::from("/no/such/key");
        let cert = PathBuf::from("/no/such/cert");
        let res = validate_ca(key, cert).unwrap();
        assert!(
            res.is_none(),
            "should return None, indicating not configured"
        );
    }

    #[test]
    fn process_response_when_error_should_not_contain_the_word_error() {
        let error =
            process_client_response(Ok(Response::Error("some error".to_string()))).unwrap_err();
        let result = format!("{}", error);
        assert!(!result.contains("ERROR"));
    }

    #[test]
    fn test_link_input_with_name_and_dir() {
        let web_dir = PathBuf::from("/tmp/");
        let web_dir = std::fs::canonicalize(web_dir).unwrap();
        let (client, path, name) =
            parse_link_input(Some("name".to_string()), Some(web_dir.clone()));
        assert_eq!(web_dir, path);
        let mut target = client.state_dir;
        target.push("name");
        assert_eq!(name, target.to_str().unwrap());
    }

    #[test]
    fn test_link_input_with_only_name() {
        let current = std::env::current_dir().unwrap();
        let (client, path, name) = parse_link_input(Some("name".to_string()), None);
        let mut target = client.state_dir;
        target.push("name");
        assert_eq!(name, target.to_str().unwrap());
        assert_eq!(path, std::fs::canonicalize(current).unwrap());
    }

    #[test]
    fn test_link_input_with_no_input() {
        let current = std::env::current_dir().unwrap();
        let the_name = current.file_name().unwrap();
        let (client, path, name) = parse_link_input(None, None);
        let mut target = client.state_dir;
        target.push(the_name);
        assert_eq!(name, target.to_str().unwrap());
        assert_eq!(path, std::fs::canonicalize(current).unwrap());
    }
}
