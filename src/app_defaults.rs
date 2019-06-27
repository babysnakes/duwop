use std::path::PathBuf;

pub const DNS_PORT: u16 = 9053;
pub const HTTP_PORT: u16 = 80;
pub const MANAGEMENT_PORT: u16 = 9054;
pub const STATE_DIR_RELATIVE: &str = ".duwop/state";
pub const LOG_DIR: &str = ".duwop/logs";
pub const VERSION: &str = "0.1-beta";
pub const LOG_LEVEL: &str = "duwop:info";

pub fn state_dir() -> PathBuf {
    let mut default_state_dir = dirs::home_dir().expect("Couldn't extract home directory");
    default_state_dir.push(STATE_DIR_RELATIVE);
    default_state_dir
}
