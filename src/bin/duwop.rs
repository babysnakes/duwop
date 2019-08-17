use duwop::app_defaults::*;
use duwop::cli_helpers::*;
use duwop::dns::DNSServer;
use duwop::management::Server as ManagementServer;
use duwop::state::AppState;
use duwop::web::Server as WebServer;

use std::path::PathBuf;
use std::sync::{Arc, RwLock};

use dotenv;
use failure::Error;
use flexi_logger::{Cleanup, Criterion, Logger, Naming};
use futures::future::{self, Future};
use log::{debug, error, info};
use structopt::{self, StructOpt};

/// Web serve local directories and proxy local ports on default http port and
/// real DNS names.
#[derive(Debug, StructOpt)]
#[structopt(name = "duwop", author = "")]
struct Cli {
    /// alternative DNS port
    #[structopt(long = "dns-port", value_name = "PORT", env = "DUWOP_DNS_PORT")]
    dns_port: Option<u16>,

    /// alternative HTTP port
    #[structopt(
        name = "http-port",
        long = "http-port",
        value_name = "PORT",
        env = "DUWOP_HTTP_PORT"
    )]
    http_port: Option<u16>,

    /// alternative HTTPS port
    #[structopt(
        name = "https-port",
        long = "https-port",
        value_name = "PORT",
        env = "DUWOP_HTTPS_PORT"
    )]
    https_port: Option<u16>,

    /// alternative management port
    #[structopt(long = "mgmt-port", value_name = "PORT", env = "DUWOP_MANAGEMENT_PORT")]
    management_port: Option<u16>,

    // development only, hidden
    #[structopt(long = "state-dir", hidden = true, env = "DUWOP_APP_STATE_DIR")]
    state_dir: Option<PathBuf>,

    // mostly for development. hidden.
    #[structopt(long = "custom-log", hidden = true, env = "DUWOP_LOG_LEVEL")]
    custom_log_level: Option<String>,

    /// Log to file instead of stderr, on by default when using as service (launchd)
    #[structopt(long = "log-to-file")]
    log_to_file: bool,

    /// Enable launchd socket (for running on mac in port 80)
    #[structopt(long = "launchd", conflicts_with = "http-port")]
    launchd: bool,

    /// disable TLS - service will ony serve through HTTP
    #[structopt(long = "disable-tls")]
    disable_tls: bool,
} // FIX disable tls

fn main() {
    dotenv::dotenv().ok();
    let app = Cli::from_args();
    match run(app) {
        Ok(_) => {
            info!("Stopping...");
        }
        Err(err) => {
            print_full_error(err);
            std::process::exit(1);
        }
    }
}

fn run(app: Cli) -> Result<(), Error> {
    let log_level = app
        .custom_log_level
        .clone()
        .unwrap_or_else(|| LOG_LEVEL.to_owned());
    // TODO: can we do it (enable log to file if launchd) automatically with clap?
    let log_handler = if app.log_to_file || app.launchd {
        Logger::with_str(&log_level)
            .log_to_file()
            .directory(LOG_DIR.to_owned())
            .rotate(
                Criterion::Size(10_000_000),
                Naming::Numbers,
                Cleanup::KeepLogFiles(4),
            )
            .start()
            .unwrap()
    } else {
        Logger::with_str(&log_level).start().unwrap()
    };
    info!("Starting...");
    debug!("running with options: {:#?}", app);
    let locked_handler = Arc::new(RwLock::new(log_handler));
    let state_dir = app.state_dir.unwrap_or_else(|| STATE_DIR.to_owned());
    let mut app_state = AppState::new(&state_dir);
    app_state.load_services()?;
    let locked = Arc::new(RwLock::new(app_state));
    let dns_server = DNSServer::new(app.dns_port.unwrap_or(DNS_PORT))?;
    // this is a hack until I completely rewrite initialization.
    let mut service_to_spawn = vec![];
    let web_server = WebServer::new_http(
        app.http_port.unwrap_or(HTTP_PORT),
        app.launchd,
        Arc::clone(&locked),
    )?
    .run();
    service_to_spawn.push(web_server);
    if !app.disable_tls {
        let web_server_ssl = WebServer::new_https(
            app.https_port.unwrap_or(HTTPS_PORT),
            app.launchd,
            Arc::clone(&locked),
        )?
        .run();
        service_to_spawn.push(web_server_ssl);
    }
    let management_server = ManagementServer::new(
        app.management_port.unwrap_or(MANAGEMENT_PORT),
        Arc::clone(&locked),
        Arc::clone(&locked_handler),
        log_level,
    )
    .run();
    service_to_spawn.push(management_server);
    tokio::run(future::lazy(|| {
        tokio::spawn(dns_server.map_err(|err| {
            error!("DNS Server error: {:?}", err);
        }));
        for service in service_to_spawn {
            tokio::spawn(service);
        }
        Ok(())
    }));
    Ok(())
}
