use std::collections::HashMap;
use std::fs::File;
use std::io::{BufReader, Result};
use std::sync::{Arc, RwLock};

use log::{debug};
use serde::{Deserialize, Serialize};
use serde_json;

#[derive(Deserialize, Serialize, Debug)]
pub enum ServiceType {
  StaticFiles(String),
}

pub struct AppState {
  pub services: HashMap<String, ServiceType>,
}

impl AppState {
  pub fn load(path: &str) -> Result<Arc<RwLock<AppState>>> {
    let mut state = AppState {
      services: HashMap::new(),
    };
    &state.load_from_saved(path)?;
    Ok(Arc::new(RwLock::new(state)))
  }

  pub fn load_from_saved(&mut self, path: &str) -> Result<()> {
    let f = File::open(path)?;
    let reader = BufReader::new(f);
    self.services = serde_json::from_reader(reader)?;
    debug!("state: {:#?}", &self.services);
    Ok(())
  }
}
