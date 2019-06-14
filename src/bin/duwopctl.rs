use duwop::app_defaults::*;
use duwop::cli_helpers::*;
use duwop::management::client::Client as MgmtClient;
use duwop::management::{LogLevel, Request, Response};

use clap::{arg_enum, value_t_or_exit, App, AppSettings, Arg, SubCommand};
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
    Reload {
        mgmt_port: u16,
    },
    Log {
        mgmt_port: u16,
        cmd: LogCommand,
        custom_level: Option<String>,
    },
}

arg_enum! {
    #[derive(Debug, PartialEq)]
    enum LogCommand {
        Debug,
        Trace,
        Reset,
        Custom,
    }
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
    let log_level_opt = "log-level";
    let custom_level_opt = "custom-level";

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
        .subcommands(vec![
            SubCommand::with_name("reload")
                .about("Reload duwop configuration from disk")
                .arg(&management_port_arg),
            SubCommand::with_name("log")
                .about("Modify log level on duwop service")
                .arg(&management_port_arg.global(true))
                .args(&[
                    Arg::with_name(log_level_opt)
                        .help("Log level to switch the service to (reset to return to default)")
                        .required(true)
                        .possible_values(&LogCommand::variants())
                        .case_insensitive(true),
                    Arg::with_name(custom_level_opt)
                        .help("custom log level (e.g. trace,tokio:warn). valid only if log-level is 'custom'")
                        .required_if(&log_level_opt, "custom"),
                ])
        ])
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
        ("log", Some(cmd_m)) => {
            let cmd = value_t_or_exit!(cmd_m, log_level_opt, LogCommand);
            let custom_level = cmd_m.value_of(custom_level_opt).map(String::from);
            subcommand = Command::Log {
                mgmt_port: parse_val_with_default::<u16>(
                    management_port_opt,
                    &cmd_m,
                    MANAGEMENT_PORT,
                ),
                cmd,
                custom_level,
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
        Command::Log {
            mgmt_port,
            cmd,
            custom_level,
        } => run_log_command(mgmt_port, cmd, custom_level),
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

fn run_log_command(port: u16, cmd: LogCommand, custom_level: Option<String>) -> Result<(), Error> {
    let client = MgmtClient::new(port);
    let request = match cmd {
        LogCommand::Debug => Request::SetLogLevel(LogLevel::DebugLevel),
        LogCommand::Trace => Request::SetLogLevel(LogLevel::TraceLevel),
        LogCommand::Custom => {
            let level = custom_level.unwrap(); // should be protected by clap 'requires'
            Request::SetLogLevel(LogLevel::CustomLevel(level))
        }
        LogCommand::Reset => Request::ResetLogLevel,
    };
    process_client_response(client.run_client_command(request))
}
