mod dns;
mod state;
mod web;

use dns::DNSServer;
use state::AppState;

use std::env;

use dotenv;
use env_logger;
use failure::{Error};
use futures::future::{self, Future};
use log::{error, info};

fn main() {
    match run() {
        Ok(_) => {
            info!("Stopping...");
        },
        Err(err) => {
            error!("{}", err);
            for cause in err.iter_causes() {
                error!("{}", cause);
            }
        }
    }
}

fn run() -> Result<(), Error> {
    dotenv::dotenv().ok();
    env_logger::init();
    info!("Starting...");
    let path = env::var("APP_STATE_DB")?;
    let dns_port_env = env::var("DNS_PORT").unwrap_or("8053".to_string());
    let dns_port: u16 = dns_port_env.parse()?;
    let http_port_env = env::var("HTTP_PORT").unwrap_or("80".to_string());
    let http_port: u16 = http_port_env.parse()?;
    let app_state = AppState::load(&path)?;
    let dns_server = DNSServer::new(dns_port)?;
    let web_server = web::new_server(http_port, app_state);
    tokio::run(future::lazy(|| {
        tokio::spawn(dns_server.map_err(|err| {
            error!("DNS Server error: {:?}", err);
        }));
        tokio::spawn(web_server);
        Ok(())
    }));
    Ok(())
}