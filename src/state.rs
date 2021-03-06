use std::collections::HashMap;
use std::ffi::OsString;
use std::fs::{read_dir, File};
use std::io::{BufRead, BufReader};
use std::net::{Ipv4Addr, SocketAddr};
use std::path::{Path, PathBuf};
use std::str::FromStr;

use failure::{format_err, Error, ResultExt};
use log::{debug, info, trace, warn};
use yansi::Paint;

#[derive(Debug, PartialEq)]
pub enum ServiceType {
    StaticFiles(PathBuf),
    ReverseProxy(SocketAddr),
    /// A file with problem - e.g. filename is not UTF-8 or socket address is not valid.
    InvalidConfig(String),
}

impl ServiceType {
    fn parse_config<P: AsRef<Path>>(path: P) -> std::io::Result<Self> {
        if path.as_ref().is_dir() {
            debug!("found directory {:?}", path.as_ref());
            std::fs::canonicalize(path).map(|path| Ok(ServiceType::StaticFiles(path)))?
        } else {
            debug!("parsing file {:?}", path.as_ref());
            let first_line = read_first_line_from_file(path)?;
            let mut parts = first_line.splitn(2, ':');
            match parts.next() {
                Some("proxy") => Ok(ServiceType::parse_proxy(parts.next())),
                Some(directive) => {
                    warn!("found invalid directive in config file: '{}'", directive);
                    Ok(ServiceType::InvalidConfig(format!(
                        "invalid directive: '{}'",
                        directive
                    )))
                }
                None => Ok(ServiceType::InvalidConfig("missing directive".to_string())),
            }
        }
    }

    /// Parses the optionally supplied socket address, but returns address
    /// containing localhost and the port extracted for the address. This might
    /// change in the future (hence the use of full SocketAddr instead of just
    /// port).
    fn parse_proxy(addr_option: Option<&str>) -> Self {
        match addr_option {
            Some(addr_str) => match SocketAddr::from_str(addr_str) {
                Ok(addr) => ServiceType::ReverseProxy((Ipv4Addr::LOCALHOST, addr.port()).into()),
                Err(err) => {
                    warn!("error parsing address ({}): {}", addr_str, err);
                    ServiceType::InvalidConfig(format!(
                        "not a valid <host:port> address: {}",
                        addr_str
                    ))
                }
            },
            None => ServiceType::InvalidConfig("missing socket address".to_string()),
        }
    }

    pub fn create(self, path: &dyn AsRef<Path>) -> Result<(), Error> {
        if path.as_ref().exists() {
            return Err(format_err!("refuses to overwrite existing file"));
        }
        let name = path
            .as_ref()
            .file_name()
            .ok_or_else(|| format_err!("Empty service name"))?;
        match self {
            ServiceType::StaticFiles(source) => {
                std::os::unix::fs::symlink(&source, path)
                    .context("linking web directory")
                    .map_err(Error::from)?;
                #[allow(clippy::unit_arg)]
                Ok(info!(
                    "created static file service: {:?} pointing to {:?}",
                    &name, &source
                ))
            }
            ServiceType::ReverseProxy(addr) => {
                std::fs::write(
                    &path,
                    format!(
                        "proxy:{}\n# Note: the IP is currently ignored, only port is used.",
                        addr.to_string()
                    ),
                )
                .context(format!("writing socket address to {:?}", &path.as_ref()))?;
                info!(
                    "created proxy service: {:?} pointing at: {}",
                    &path.as_ref(),
                    &addr.to_string(),
                );
                Ok(())
            }
            ServiceType::InvalidConfig(_) => Err(format_err!(
                "it does not make sense to create invalid config"
            )),
        }
    }

    pub fn pprint(&self, name: &str) {
        let wrapper = textwrap::Wrapper::with_termwidth()
            .initial_indent("    -> ")
            .subsequent_indent("       ");
        match self {
            ServiceType::StaticFiles(path) => {
                let fallback_path = format!("{:?}", &path);
                let path_str = path.to_str().unwrap_or(&fallback_path);
                println!(
                    "* {} [Static Files Directory]:\n{}",
                    Paint::green(&name),
                    wrapper.fill(path_str)
                );
            }
            ServiceType::ReverseProxy(addr) => {
                println!(
                    "* {} [Reverse Proxy]:\n{}",
                    Paint::green(&name),
                    wrapper.fill(&format!("http://{}", addr.to_string()))
                );
            }
            ServiceType::InvalidConfig(msg) => {
                println!(
                    "* {} [Config Error]:\n{}",
                    Paint::red(&name),
                    wrapper.fill(msg)
                );
            }
        }
    }
}

#[derive(Debug, PartialEq)]
pub enum ServiceConfigError {
    NameError(OsString),
    IoError(String),
}

#[derive(Debug)]
pub struct AppState {
    pub services: HashMap<String, ServiceType>,
    errors: Vec<ServiceConfigError>,
    path: PathBuf,
}

impl AppState {
    /// Returns AppState with empty services. use `load_services` to populate
    /// the services from disk.
    pub fn new(path: &PathBuf) -> Self {
        AppState {
            path: path.to_path_buf(),
            errors: vec![],
            services: HashMap::new(),
        }
    }

    // Currently this function is a helper for tests
    #[cfg(test)]
    pub fn from_services(services: HashMap<String, ServiceType>) -> Self {
        AppState {
            services,
            errors: vec![],
            path: PathBuf::new(),
        }
    }

    pub fn load_services(&mut self) -> Result<(), Error> {
        info!("loading state from file system");
        debug!("loading services from {:?}", &self.path);
        let mut services = HashMap::new();
        let mut errors = vec![];

        for entry in read_dir(&self.path).context(format!(
            "reading state directory ({:?})",
            &self.path.to_path_buf().into_os_string()
        ))? {
            debug!("parsing entry: {:?}", entry);
            let entry = entry?;
            let name = entry.file_name();
            let path = entry.path();
            match name.clone().into_string() {
                Ok(key) => match ServiceType::parse_config(path) {
                    Ok(service_type) => {
                        services.insert(key, service_type);
                    }
                    Err(err) => {
                        warn!("error parsing {:?}: {}", name, err);
                        errors.push(ServiceConfigError::IoError(format!("{:?}: {}", name, err)));
                    }
                },
                Err(_) => {
                    warn!("encountered a non utf-8 filename: {:?}", name.clone());
                    errors.push(ServiceConfigError::NameError(name));
                }
            }
        }

        self.errors = errors;
        self.services = services;
        trace!("parsed state: {:#?}", &self);
        Ok(())
    }

    pub fn errors(&self) -> &Vec<ServiceConfigError> {
        &self.errors
    }
}

fn read_first_line_from_file<P: AsRef<Path>>(path: P) -> std::io::Result<String> {
    let f = File::open(&path)?;
    let mut rdr = BufReader::new(f);
    let mut first_line = String::new();
    rdr.read_line(&mut first_line)?;
    Ok(first_line.trim_end().to_owned())
}

#[cfg(test)]
mod tests {
    use super::*;
    use assert_fs::prelude::*;

    #[test]
    fn parse_config_extracts_links_from_directories() {
        let source_dir = std::env::current_dir().unwrap();
        // TODO: this will fail if working directory is more then one level of
        // symlink, hopefully it won't happen.
        let source_path = match source_dir.read_link() {
            Ok(path) => path,
            Err(_) => source_dir,
        };
        let state_dir = assert_fs::TempDir::new().unwrap();
        let mut link_path = state_dir.path().to_path_buf();
        link_path.push("test");
        std::os::unix::fs::symlink(&source_path, &link_path).unwrap();
        assert_eq!(
            ServiceType::parse_config(link_path).unwrap(),
            ServiceType::StaticFiles(source_path)
        );
        state_dir.close().unwrap();
    }

    #[test]
    fn parse_config_reads_proxy_files_correctly() {
        let file = assert_fs::NamedTempFile::new("proxyfile").unwrap();
        let addr_str = "127.0.0.1:8080";
        file.write_str(&format!("proxy:{}", addr_str)).unwrap();
        assert_eq!(
            ServiceType::parse_config(file.path()).unwrap(),
            ServiceType::ReverseProxy(SocketAddr::from_str(addr_str).unwrap())
        );
    }

    #[test]
    fn parse_config_returns_invalid_config_if_proxy_with_invalid_socket_addr() {
        let file = assert_fs::NamedTempFile::new("proxyfile").unwrap();
        file.write_str("proxy:localhost").unwrap();
        match ServiceType::parse_config(file.path()) {
            Ok(ServiceType::InvalidConfig(e)) => {
                assert!(
                    e.contains("not a valid <host:port>"),
                    "wrong InvalidConfig message"
                );
            }
            other => panic!("bad response ({:?}) from parse_config", other),
        };
    }

    #[test]
    fn parse_config_returns_invalid_config_if_proxy_with_no_socket_addr() {
        let file = assert_fs::NamedTempFile::new("proxyfile").unwrap();
        file.write_str("proxy").unwrap();
        match ServiceType::parse_config(file.path()) {
            Ok(ServiceType::InvalidConfig(e)) => {
                assert!(
                    e.contains("missing socket address"),
                    "wrong InvalidConfig message"
                );
            }
            other => panic!("returned bad response: {:?}", other),
        };
    }

    #[test]
    fn parse_config_tags_unknown_directive_as_invalid_config() {
        let file = assert_fs::NamedTempFile::new("proxyfile").unwrap();
        file.write_str("wrong:something").unwrap();
        match ServiceType::parse_config(file.path()) {
            Ok(ServiceType::InvalidConfig(e)) => {
                assert!(
                    e.contains("invalid directive"),
                    "wrong InvalidConfig message"
                );
            }
            other => panic!("returned bad response: {:?}", other),
        };
    }

    #[test]
    fn service_type_create_refuses_to_overwrite_existing_file() {
        let file = assert_fs::NamedTempFile::new("new file").unwrap();
        file.write_str("something").unwrap();
        let service_type = ServiceType::ReverseProxy(([127, 0, 0, 1], 1000).into());
        match service_type.create(&file.path().to_path_buf()) {
            Ok(_) => panic!("overwriting file should have failed"),
            Err(err) => assert!(err.to_string().contains("overwrite")),
        }
    }

    #[test]
    fn parse_config_ignores_proxy_host_and_always_refers_to_localhost() {
        let file = assert_fs::NamedTempFile::new("proxyfile").unwrap();
        file.write_str("proxy:10.0.0.1:9999").unwrap();
        match ServiceType::parse_config(file.path()) {
            Ok(ServiceType::ReverseProxy(addr)) => {
                assert_eq!(addr, ([127, 0, 0, 1], 9999).into());
            }
            other => panic!("returned bad response: {:?}", other),
        };
    }
}
