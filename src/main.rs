mod dns_server;

use dns_server::DNSServer;

fn main() {
    println!("Starting...");
    let server = DNSServer { port: 8053 };
    server.run();
}
