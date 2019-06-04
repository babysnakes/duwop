use duwop::app_defaults::{DNS_PORT, HTTP_PORT, MANAGEMENT_PORT, STATE_DIR};
use duwop::cli_helpers::*;
use duwop::dns::DNSServer;
use duwop::management::Server as ManagementServer;
use duwop::state::AppState;
use duwop::web;

use std::path::PathBuf;
use std::sync::{Arc, RwLock};

use clap::{App, Arg};
use dotenv;
use env_logger;
use failure::Error;
use futures::future::{self, Future};
use log::{debug, error, info};

#[derive(Debug)]
struct Opt {
    dns_port: u16,
    web_port: u16,
    management_port: u16,
    state_dir: PathBuf,
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
    let mut default_state_dir = dirs::home_dir().expect("Couldn't extract home directory");
    default_state_dir.push(STATE_DIR);
    let dns_port_opt = "dns-port";
    let http_port_opt = "http-port";
    let management_port_opt = "mgmt-port";
    let state_dir_opt = "state-dir";
    let launchd_opt = "launchd";

    let matches = App::new("duwop")
        .version("0.1-alpha")
        .about("Web serve local directories and proxy local ports on default http port and real DNS names.")
        .args(&[
            Arg::with_name(dns_port_opt)
                .long(dns_port_opt)
                .help("Alternative DNS port")
                .value_name("PORT")
                .takes_value(true)
                .env("DUWOP_DNS_PORT"),
            Arg::with_name(http_port_opt)
                .long(http_port_opt)
                .help("Alternative HTTP port")
                .value_name("PORT")
                .takes_value(true)
                .env("DUWOP_HTTP_PORT"),
            Arg::with_name(management_port_opt)
                .long(management_port_opt)
                .help("Alternative management port")
                .value_name("PORT")
                .takes_value(true)
                .env("DUWOP_MANAGEMENT_PORT"),
            // Development only. Not for regular use.
            Arg::with_name(state_dir_opt)
                .long(state_dir_opt)
                .hidden(true)
                .takes_value(true)
                .env("DUWOP_APP_STATE_DIR"),
            Arg::with_name(launchd_opt)
                .long(launchd_opt)
                .conflicts_with(http_port_opt)
                .help("Enable launchd socket (for running on mac in port 80)"),
        ])
        .get_matches();

    Opt {
        dns_port: parse_val_with_default::<u16>(dns_port_opt, &matches, DNS_PORT),
        web_port: parse_val_with_default::<u16>(http_port_opt, &matches, HTTP_PORT),
        management_port: parse_val_with_default::<u16>(
            management_port_opt,
            &matches,
            MANAGEMENT_PORT,
        ),
        state_dir: parse_val_with_default::<PathBuf>(state_dir_opt, &matches, default_state_dir),
        launchd: matches.is_present(launchd_opt),
    }
}

fn run(opt: Opt) -> Result<(), Error> {
    info!("Starting...");
    debug!("running with options: {:#?}", opt);
    let mut app_state = AppState::new(&opt.state_dir);
    app_state.load_services()?;
    let locked = Arc::new(RwLock::new(app_state));
    let dns_server = DNSServer::new(opt.dns_port)?;
    let web_server = web::new_server(opt.web_port, opt.launchd, Arc::clone(&locked));
    let management_server = ManagementServer::as_future(opt.management_port, Arc::clone(&locked));
    tokio::run(future::lazy(|| {
        tokio::spawn(dns_server.map_err(|err| {
            error!("DNS Server error: {:?}", err);
        }));
        tokio::spawn(web_server);
        tokio::spawn(management_server);
        Ok(())
    }));
    Ok(())
}
