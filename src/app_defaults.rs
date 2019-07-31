use lazy_static::lazy_static;
use std::path::PathBuf;

pub const DNS_PORT: u16 = 9053;
pub const HTTP_PORT: u16 = 80;
pub const HTTPS_PORT: u16 = 443;
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
/// The name of the HTTPS socket in launchd agent file
pub const LAUNCHD_TLS_SOCKET: &str = "DuwopTlsSocket";

pub const TLS_ENTRY_C: &str = "IL";
pub const TLS_ENTRY_ST: &str = "Israel";
pub const TLS_ENTRY_O: &str = "Duwop IO";
pub const TLS_ENTRY_CN: &str = "Duwop Test";

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
  pub static ref CA_DIR: PathBuf = {
    let mut dir = HOME_DIR.clone();
    dir.push("Library/Application Support/io.duwop");
    dir
  };
  pub static ref CA_KEY: PathBuf = {
    let mut path = CA_DIR.clone();
    path.push("key.pem");
    path
  };
  pub static ref CA_CERT: PathBuf = {
    let mut path = CA_DIR.clone();
    path.push("cert.pem");
    path
  };
  static ref CERTS_DIR: PathBuf = {
    let mut dir = DUWOP_DIR.clone();
    dir.push("ssl");
    dir
  };
  /// Default certificate file
  pub static ref CERT_FILE: PathBuf = {
    let mut file = CERTS_DIR.clone();
    file.push("duwop.crt");
    file
  };
  /// Default public key file.
  pub static ref PRIV_KEY: PathBuf = {
    let mut file = CERTS_DIR.clone();
    file.push("duwop.key");
    file
  };
}

/// Construct a path in home directory _relative_ to home directory. THe provided
/// path **must not** start with a slash.
pub fn in_home_dir(path: &str) -> PathBuf {
    let mut dir = HOME_DIR.clone();
    dir.push(path);
    dir
}
