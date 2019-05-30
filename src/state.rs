use std::collections::HashMap;
use std::ffi::OsString;
use std::fs::{read_dir, DirEntry, File};
use std::io::{BufRead, BufReader};
use std::path::PathBuf;

use failure::{format_err, Error, ResultExt};
use log::{debug, info, trace, warn};
use url::Url;

#[derive(Debug, PartialEq)]
pub enum ServiceType {
    StaticFiles(OsString),
    ReverseProxy(Url),
    /// A file that is not parsed as state - used for cleanup, etc.
    InvalidFile,
    /// A file with problem - e.g. filename is not UTF-8 or url is not valid.
    InvalidConfig(String),
}

#[derive(Debug)]
pub struct AppState {
    pub services: HashMap<String, ServiceType>,
    path: PathBuf,
}

impl AppState {
    /// Returns a Result<AppState, Error, without any services defined. Use
    /// `load` to populate services from state directory.
    pub fn new(path: Option<String>) -> Result<Self, Error> {
        let state_dir = match path {
            Some(path) => PathBuf::from(path),
            None => {
                dirs::home_dir().ok_or_else(|| format_err!("Couldn't extract home directory"))?
            }
        };
        Ok(AppState {
            path: state_dir,
            services: HashMap::new(),
        })
    }

    // Currently this function is a helper for tests
    #[cfg(test)]
    pub fn from_services(services: HashMap<String, ServiceType>) -> Self {
        AppState {
            services,
            path: PathBuf::new(),
        }
    }

    pub fn load_services(&mut self) -> Result<(), Error> {
        info!("loading state from file system");
        debug!("loading services from {:?}", &self.path);
        let services = read_dir(&self.path)?
            .filter_map(|item| {
                let entry = item.unwrap();
                let name = entry.file_name();
                let path = entry.path();
                match (name.clone().into_string(), path.is_dir(), path.extension()) {
                    (Err(_), _, _) => {
                        // not sure it's even possible to create non utf filenames...
                        warn!(
                            "unsupported (NON UTF) file name: ({:?})! You have to delete it manually!",
                            path.into_os_string(),
                        );
                        None
                    },
                    (Ok(key), true, _) => {
                        debug!("found directory: {}", key);
                        match extract_path_of_symlink_or_file(entry) {
                            Ok(path) => {
                                debug!("resolved directory to {:?}", path);
                                Some((key, ServiceType::StaticFiles(path)))
                            },
                            Err(e) => {
                                let message = format!("error extracting directory info: {}", e);
                                warn!("{}", message);
                                Some((key, ServiceType::InvalidConfig(message)))
                            },
                        }
                    }
                    (Ok(key), false, Some(ext)) => match ext.to_str() {
                        Some("proxy") if key.len() > 6 => {
                            let short_key = &key[..key.len() - 6];
                            match extract_url_from_proxy_file(path) {
                            Ok(url) => {
                                debug!("found proxy {} pointing at {}", key, url);
                                Some((short_key.to_string(), ServiceType::ReverseProxy(url)))
                            }
                            Err(e) => {
                                let message = format!("error parsing Url from '{}': {}", key, e);
                                warn!("{}", message);
                                Some((short_key.to_string(), ServiceType::InvalidConfig(message)))
                            },
                        }},
                        _ => {
                            warn!("found unsupported file '{}', ignoring.", key);
                            Some((key, ServiceType::InvalidFile))
                        },
                    },
                    (Ok(key), false, _) => {
                        warn!("found unsupported file '{}', ignoring.", key);
                        Some((key, ServiceType::InvalidFile))
                    },
                }
            })
            .collect::<HashMap<String, ServiceType>>();
        trace!("loaded services: {:#?}", services);
        self.services = services;
        Ok(())
    }
}

/// Reads the **first** line of the supplied path and tries to parse it as Url.
/// Other lines are ignored.
fn extract_url_from_proxy_file(path: PathBuf) -> Result<Url, Error> {
    let f = File::open(&path).context(format!(
        "opening proxy config file '{:?}'",
        &path.as_os_str()
    ))?;
    let mut rdr = BufReader::new(f);
    let mut first_line = String::new();
    rdr.read_line(&mut first_line)
        .context(format!("reading first line of '{:?}", &path.as_os_str()))?;
    Url::parse(&first_line)
        .context(format!(
            "parsing url from '{}' (from file: {:?}",
            &first_line,
            &path.as_os_str()
        ))
        .map_err(Error::from)
}

fn extract_path_of_symlink_or_file(entry: DirEntry) -> Result<OsString, Error> {
    let file_type = entry
        .file_type()
        .context(format!("extracting info on {:?}", entry.file_name()))?;
    if file_type.is_symlink() {
        let linked = entry.path().read_link()?;
        linked
            .canonicalize()
            .context(format!("resolving symlink {:?}", entry.file_name()))?;
        Ok(linked.into_os_string())
    } else {
        warn!(
            "{:?} is not a symbolic link. It's better to define symbolic link!",
            entry.file_name()
        );
        Ok(entry.path().into_os_string())
    }
}
