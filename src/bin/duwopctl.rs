use duwop::app_defaults::*;
use duwop::cli_helpers::*;
use duwop::management::client::Client as MgmtClient;
use duwop::management::{Request, Response};

use clap::{App, AppSettings, Arg, SubCommand};
use dotenv;
use failure::{format_err, Error};
use flexi_logger;
use log::{debug, error, info};

#[derive(Debug)]
struct Opt {
    command: Command,
}

#[derive(Debug)]
enum Command {
    Reload { mgmt_port: u16 },
}

fn main() {
    dotenv::dotenv().ok();
    flexi_logger::Logger::with_env().start().unwrap();
    let opt = parse_options();
    match run(opt) {
        Ok(_) => {}
        Err(err) => {
            error!("{}", err);
            for cause in err.iter_causes() {
                error!("{}", cause);
            }
        }
    }
}

fn parse_options() -> Opt {
    let management_port_opt = "mgmt-port";

    let management_port_arg = Arg::with_name(management_port_opt)
        .long(management_port_opt)
        .help("alternative management port")
        .value_name("PORT")
        .takes_value(true)
        .env("DUWOP_MANAGEMENT_PORT");

    let matches = App::new("duwopctl")
        .setting(AppSettings::SubcommandRequiredElseHelp)
        .version(VERSION)
        .about("Configure/Manage duwop service.")
        .subcommands(vec![SubCommand::with_name("reload")
            .about("Reload duwop configuration from disk")
            .arg(management_port_arg)])
        .get_matches();

    let subcommand: Command;
    match matches.subcommand() {
        ("reload", Some(cmd_m)) => {
            subcommand = Command::Reload {
                mgmt_port: parse_val_with_default::<u16>(
                    management_port_opt,
                    &cmd_m,
                    MANAGEMENT_PORT,
                ),
            };
        }
        _ => unreachable!(), // we use SubCommand::Required.
    }
    debug!("subcommand: {:?}", subcommand);
    Opt {
        command: subcommand,
    }
}

fn run(opt: Opt) -> Result<(), Error> {
    debug!("running with options: {:#?}", opt);
    match opt.command {
        Command::Reload { mgmt_port } => run_reload(mgmt_port),
    }
}

fn process_client_response(result: Result<Response, Error>) -> Result<(), Error> {
    match result {
        Ok(resp) => {
            let msg = resp.serialize();
            match resp {
                Response::Done => {
                    info!("{}", msg);
                    Ok(())
                }
                Response::Error(_) => Err(format_err!("Error from server: {}", msg)),
            }
        }
        Err(err) => Err(err),
    }
}

fn run_reload(port: u16) -> Result<(), Error> {
    let client = MgmtClient::new(port);
    process_client_response(client.run_client_command(Request::ReloadState))
}
