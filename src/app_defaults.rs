use std::path::PathBuf;

pub const DNS_PORT: u16 = 9053;
pub const HTTP_PORT: u16 = 80;
pub const MANAGEMENT_PORT: u16 = 9054;
pub const STATE_DIR_RELATIVE: &str = ".duwop/state";
pub const LOG_DIR: &str = ".duwop/logs";
pub const LOG_LEVEL: &str = "duwop=info";
pub const LAUNCH_AGENTS_DIR: &str = "Library/LaunchAgents/";
pub const AGENT_NAME: &str = "org.babysnakes.duwop";
pub const RESOLVER_DIR: &str = "/etc/resolver/";
pub const RESOLVER_FILE: &str = "test";
pub const LAUNCHD_SOCKET: &str = "DuwopSocket";

pub fn state_dir() -> PathBuf {
    let mut default_state_dir = dirs::home_dir().expect("Couldn't extract home directory");
    default_state_dir.push(STATE_DIR_RELATIVE);
    default_state_dir
}
