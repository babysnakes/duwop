mod dns;
mod state;

use dns::DNSServer;
use state::AppState;

use std::env;

use dotenv;
use env_logger;
use futures::future::{self, Future};
use log::{error, info};

fn main() {
    dotenv::dotenv().ok();
    env_logger::init();
    info!("Starting...");
    let path = env::var("APP_STATE_DB").unwrap();
    let dns_port_env = env::var("DNS_PORT").unwrap_or("8053".to_string());
    let dns_port: u16 = dns_port_env.parse().unwrap();
    let _app_state = AppState::load(&path);
    let dns_server = DNSServer::new(dns_port);
    tokio::run(future::lazy(|| {
        tokio::spawn(dns_server.map_err(|err| {
            error!("DNS Server error: {:?}", err);
        }));
        Ok(())
    }));
}
