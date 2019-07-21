use lazy_static::lazy_static;
use std::path::PathBuf;

pub const DNS_PORT: u16 = 9053;
pub const HTTP_PORT: u16 = 80;
pub const MANAGEMENT_PORT: u16 = 9054;
pub const LOG_LEVEL: &str = "duwop=info";
/// The name of the launchd agent
pub const AGENT_NAME: &str = "org.babysnakes.duwop";
/// on macos, the directory that contains custom resolver files
pub const RESOLVER_DIR: &str = "/etc/resolver/";
/// We only use ".test" domain.
pub const RESOLVER_FILE: &str = "test";
/// The name of the HTTP socket in launchd agent file
pub const LAUNCHD_SOCKET: &str = "DuwopSocket";

lazy_static! {
  /// Home directory
  pub static ref HOME_DIR: PathBuf = dirs::home_dir().expect("Couldn't extract home directory");
  /// Where all things duwop live.
  pub static ref DUWOP_DIR: PathBuf = {
    let mut dir = HOME_DIR.clone();
    dir.push(".duwop");
    dir
  };
  /// Logs directory (for duwop service) - hard coded
  pub static ref LOG_DIR: PathBuf = {
    let mut dir = DUWOP_DIR.clone();
    dir.push("logs");
    dir
  };
  /// Default state directory
  pub static ref STATE_DIR: PathBuf = {
    let mut dir = DUWOP_DIR.clone();
    dir.push(".state");
    dir
  };
  /// User's launchd directory.
  pub static ref USER_LAUNCHD_DIR: PathBuf = {
    let mut dir = HOME_DIR.clone();
    dir.push("Library/LaunchAgents/");
    dir
  };
  /// The duwop launchd file path.
  pub static ref LAUNCHD_AGENT_FILE: PathBuf = {
    let mut file = USER_LAUNCHD_DIR.clone();
    let filename = format!("{}.plist", &AGENT_NAME);
    file.push(filename);
    file
  };
}
