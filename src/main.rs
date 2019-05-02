mod dns;

use dns::DNSServer;

use env_logger;
use futures::future::{self, Future};
use log::{error, info};

fn main() {
    env_logger::init();
    info!("Starting...");
    let dns_server = DNSServer::new(8053);
    tokio::run(future::lazy(|| {
        tokio::spawn(dns_server.map_err(|err| {
            error!("DNS Server error: {:?}", err);
        }));
        Ok(())
    }));
}
