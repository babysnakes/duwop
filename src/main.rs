mod dns_server;

use dns_server::DNSServer;
use env_logger;
use futures::future;
use log::info;

fn main() {
    env_logger::init();
    info!("Starting...");
    let server = DNSServer {
        port: 8053,
        subdomains: vec!["example.test".to_string()],
    }
    .run();
    tokio::run(future::lazy(|| {
        tokio::spawn(server);
        Ok(())
    }));
}
