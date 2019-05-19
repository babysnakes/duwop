use std::collections::HashMap;
use std::fs::File;
use std::io::BufReader;
use std::path::PathBuf;
use std::sync::{Arc, RwLock};

use failure::{format_err, Error, ResultExt};
use log::{debug, info};
use serde::{self, Deserialize, Serialize};
use serde_json;
use url::Url;

#[derive(Deserialize, Serialize, Debug)]
pub enum ServiceType {
    StaticFiles(String),
    #[serde(with = "url_serde")]
    ReverseProxy(Url),
}

#[derive(Debug)]
pub struct AppState {
    pub services: HashMap<String, ServiceType>,
}

impl AppState {
    pub fn load(path: &str) -> Result<Arc<RwLock<AppState>>, Error> {
        let mut state_file_path = match dirs::home_dir() {
            Some(path) => path,
            None => return Err(format_err!("Couldn't extract home directory")),
        };
        state_file_path.push(path);
        let mut state = AppState {
            services: HashMap::new(),
        };
        &state.load_from_saved(state_file_path)?;
        Ok(Arc::new(RwLock::new(state)))
    }

    pub fn load_from_saved(&mut self, path: PathBuf) -> Result<(), Error> {
        let path_name = format!("{:?}", &path);
        info!("loading state from db ({:?})", &path_name);
        let f = File::open(path).context(format!("failed to open state db ({:?})", &path_name))?;
        let reader = BufReader::new(f);
        self.services = serde_json::from_reader(reader).context("error parsing json state file")?;
        debug!("state: {:#?}", &self.services);
        Ok(())
    }
}
