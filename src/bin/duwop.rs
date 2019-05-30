use duwop::dns::DNSServer;
use duwop::state::AppState;
use duwop::web;

use std::sync::{Arc, RwLock};

use dotenv;
use env_logger;
use failure::Error;
use futures::future::{self, Future};
use log::{debug, error, info};

#[derive(Debug)]
struct Opt {
    dns_port: u16,
    web_port: u16,
    state_dir: Option<String>,
    launchd: bool,
}

fn main() {
    dotenv::dotenv().ok();
    env_logger::init();
    let opt = parse_options();
    match run(opt) {
        Ok(_) => {
            info!("Stopping...");
        }
        Err(err) => {
            error!("{}", err);
            for cause in err.iter_causes() {
                error!("{}", cause);
            }
        }
    }
}

fn parse_options() -> Opt {
    use clap::{value_t, App, Arg};

    let default_dns_port = "9053";
    let default_web_port = "80";
    let dns_port_opt = "dns-port";
    let web_port_opt = "web-port";
    let state_dir_opt = "state-dir";
    let launchd_opt = "launchd";

    let matches = App::new("duwop")
        .version("0.1-alpha")
        .about("Web serve local directories and proxy local ports on default http port and real DNS names.")
        .args(&[
            Arg::with_name(dns_port_opt)
                .long(dns_port_opt)
                .help("The dns port to listen on (UDP)")
                .value_name("PORT")
                .takes_value(true)
                .default_value(default_dns_port)
                .env("DNS_PORT"),
            Arg::with_name(web_port_opt)
                .long(web_port_opt)
                .help("The port to listen for web requests")
                .value_name("PORT")
                .takes_value(true)
                .default_value(default_web_port)
                .env("HTTP_PORT"),
            // Development only. Not for regular use.
            Arg::with_name(state_dir_opt)
                .long(state_dir_opt)
                .hidden(true)
                .takes_value(true)
                .env("APP_STATE_DIR"),
            Arg::with_name(launchd_opt)
                .long(launchd_opt)
                .help("Enable launchd socket (for running on mac in port 80)"),
        ])
        .get_matches();

    Opt {
        dns_port: value_t!(matches.value_of(dns_port_opt), u16).unwrap_or_else(|e| e.exit()),
        web_port: value_t!(matches.value_of(web_port_opt), u16).unwrap_or_else(|e| e.exit()),
        state_dir: matches.value_of(state_dir_opt).map(String::from),
        launchd: matches.is_present(launchd_opt),
    }
}

fn run(opt: Opt) -> Result<(), Error> {
    info!("Starting...");
    debug!("running with options: {:#?}", opt);
    let mut app_state = AppState::new(opt.state_dir)?;
    app_state.load_services()?;
    let locked = Arc::new(RwLock::new(app_state));
    let dns_server = DNSServer::new(opt.dns_port)?;
    let web_server = web::new_server(opt.web_port, opt.launchd, locked);
    tokio::run(future::lazy(|| {
        tokio::spawn(dns_server.map_err(|err| {
            error!("DNS Server error: {:?}", err);
        }));
        tokio::spawn(web_server);
        Ok(())
    }));
    Ok(())
}
