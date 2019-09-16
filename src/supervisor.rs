use super::dns::DNSServer;
use super::management::Server as ManagementServer;
use super::web::{HttpServer, HttpsServer};

use futures::sync::mpsc;
use futures::Future;
use log::error;
use signal_hook::iterator::Signals;
use tokio::prelude::Stream;

pub type ServerF = Box<dyn Future<Item = (), Error = ()> + Send>;

/// Helps managing all services so they could be stopped/started/restarted.
pub struct Supervisor {
    dns_server: DNSServer,
    management_server: ManagementServer,
    http_server: HttpServer,
    https_server: Option<HttpsServer>,
}

impl Supervisor {
    pub fn new(
        dns_server: DNSServer,
        management_server: ManagementServer,
        http_server: HttpServer,
        https_server: Option<HttpsServer>,
    ) -> Self {
        Supervisor {
            dns_server,
            management_server,
            http_server,
            https_server,
        }
    }

    pub fn run(self) -> Box<dyn Future<Item = (), Error = ()> + Send> {
        let (tx, rx) = mpsc::channel::<()>(1);
        let mut servers: Vec<ServerF> = vec![
            Box::new(
                self.dns_server
                    .map_err(|e| error!("DNS Server error: {}", e)),
            ),
            self.management_server.run(tx),
            self.http_server.run(),
        ];
        if let Some(server) = self.https_server {
            servers.push(server.run(rx));
        };
        let trap = Signals::new(&[signal_hook::SIGTERM, signal_hook::SIGINT])
            .unwrap()
            .into_async()
            .unwrap()
            .into_future()
            .map(|_sig| println!("Received terminate signal. Exiting..."));
        let servers = futures::future::join_all(servers)
            .map(|_| ())
            .map_err(|e| error!("error from servers: {:?}", e));
        Box::new(
            servers
                .select2(trap)
                .map_err(|_| error!("Unknown error from supervisor"))
                .map(|_| ()),
        )
    }
}
