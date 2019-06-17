use duwop::app_defaults::*;
use duwop::cli_helpers::*;
use duwop::client::*;
use duwop::management::{LogLevel, Request};

use std::path::PathBuf;

use clap::{arg_enum, value_t_or_exit, App, AppSettings, Arg, SubCommand};
use dotenv;
use failure::Error;
use flexi_logger;
use log::debug;
use url::Url;

#[derive(Debug)]
struct Opt {
    mgmt_port: u16,
    state_dir: PathBuf,
    command: Command,
}

#[derive(Debug)]
enum Command {
    Reload,
    Log {
        cmd: LogCommand,
        custom_level: Option<String>,
    },
    Link {
        web_dir: PathBuf,
        name: String,
    },
    Proxy {
        name: String,
        proxy_url: Option<Url>,
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

struct Names<'a> {
    management_port_opt: &'a str,
    state_dir_opt: &'a str,
    log_level_opt: &'a str,
    custom_level_opt: &'a str,
    link_source: &'a str,
    link_name: &'a str,
    proxy_url: &'a str,
    proxy_name: &'a str,
}

impl<'a> Names<'a> {
    fn new() -> Self {
        Names {
            management_port_opt: "mgmt-port",
            state_dir_opt: "state-dir",
            log_level_opt: "log-level",
            custom_level_opt: "custom-level",
            link_source: "directory-to-serve",
            link_name: "name",
            proxy_name: "name",
            proxy_url: "URL",
        }
    }
}

fn main() {
    dotenv::dotenv().ok();
    flexi_logger::Logger::with_env().start().unwrap();
    let names = Names::new();
    let app = create_cli_app(&names);
    match parse_options(app, &names).and_then(run) {
        Ok(_) => {}
        Err(err) => print_full_error(err),
    }
}

fn create_cli_app<'a>(names: &'a Names) -> App<'a, 'a> {
    App::new("duwopctl")
        .setting(AppSettings::SubcommandRequiredElseHelp)
        .version(VERSION)
        .about("Configure/Manage duwop service.")
        .args(&[
            Arg::with_name(names.management_port_opt)
                .long(names.management_port_opt)
                .help("alternative management port")
                .value_name("PORT")
                .global(true)
                .takes_value(true)
                .env("DUWOP_MANAGEMENT_PORT"),
            // Development only. Not for regular use.
            Arg::with_name(names.state_dir_opt)
                .long(names.state_dir_opt)
                .hidden(true)
                .takes_value(true)
                .global(true)
                .env("DUWOP_APP_STATE_DIR"),
        ])
        .subcommands(vec![
            SubCommand::with_name("reload")
                .about("Reload duwop configuration from disk"),
            SubCommand::with_name("log")
                .about("Modify log level on duwop service")
                .args(&[
                    Arg::with_name(names.log_level_opt)
                        .help("Log level to switch the service to (reset to return to default)")
                        .required(true)
                        .possible_values(&LogCommand::variants())
                        .case_insensitive(true),
                    Arg::with_name(names.custom_level_opt)
                        .help("custom log level (e.g. trace,tokio:warn). valid only if log-level is 'custom'")
                        .required_if(&names.log_level_opt, "custom"),
                ]),
            SubCommand::with_name("link")
                .about("Serve directory with web server")
                .long_about("\
                    This command will serve the specified target directory (or the \
                    current directory if none specified) with a web server accessible \
                    as http://<name>.test/. The name should not include the '.test' \
                    domain. \
                ")
                .args(&[
                    Arg::with_name(names.link_name)
                        .help("The hostname to serve it as")
                        .required(true),
                    Arg::with_name(names.link_source)
                        .help("The directory to serve, if omitted the current directory is used")
                        .required(false),
                ]),
            SubCommand::with_name("proxy")
                .about("Reverse proxy a URL")
                .long_about("\
                    This command will add configuration to reverse proxy a provided URL \
                    as http://<name>.test/. If no proxy option is provided the user will \
                    be prompted to insert one. The name should not include the `.test` \
                    domain. \
                ")
                .args(&[
                    Arg::with_name(names.proxy_name)
                        .help("The hostname to use as reverse proxy")
                        .required(true),
                    Arg::with_name(names.proxy_url)
                        .help("The URL to reverse proxy to, you will be prompted for it if not specified")
                        .required(false),
                ])
        ])
}

fn parse_options(app: App, names: &Names) -> Result<Opt, Error> {
    let matches = app.get_matches();
    let subcommand: Command;
    match matches.subcommand() {
        ("reload", Some(_)) => subcommand = Command::Reload,
        ("log", Some(cmd_m)) => {
            let cmd = value_t_or_exit!(cmd_m, names.log_level_opt, LogCommand);
            let custom_level = cmd_m.value_of(names.custom_level_opt).map(String::from);
            subcommand = Command::Log { cmd, custom_level };
        }
        ("link", Some(cmd_m)) => {
            subcommand = Command::Link {
                web_dir: std::env::current_dir()?,
                // protected by required argument.
                name: cmd_m.value_of(names.link_name).unwrap().to_string(),
            }
        }
        ("proxy", Some(cmd_m)) => {
            use clap::{Error, ErrorKind};

            let url = cmd_m.value_of(names.proxy_url).map(|url_str| {
                let msg = format!("unable to parse input ({}) as url", &url_str);
                Url::parse(url_str)
                    .map_err(|_| Error::with_description(&msg, ErrorKind::InvalidValue))
                    .unwrap_or_else(|e| e.exit())
            });
            subcommand = Command::Proxy {
                name: cmd_m.value_of(names.proxy_name).unwrap().to_string(),
                proxy_url: url,
            }
        }
        _ => unreachable!(), // we use SubCommand::Required.
    }
    debug!("subcommand: {:?}", subcommand);
    Ok(Opt {
        mgmt_port: parse_val_with_default::<u16>(
            names.management_port_opt,
            &matches,
            MANAGEMENT_PORT,
        ),
        state_dir: parse_val_with_default::<PathBuf>(names.state_dir_opt, &matches, state_dir()),
        command: subcommand,
    })
}

fn run(mut opt: Opt) -> Result<(), Error> {
    debug!("running with options: {:#?}", opt);
    match opt.command {
        Command::Reload => run_reload(opt.mgmt_port),
        Command::Log { cmd, custom_level } => run_log_command(opt.mgmt_port, cmd, custom_level),
        Command::Link { web_dir, name } => {
            opt.state_dir.push(name);
            link_web_directory(opt.state_dir, web_dir)?;
            run_reload(opt.mgmt_port)
        }
        Command::Proxy { name, proxy_url } => {
            let mut proxy_file_path = opt.state_dir;
            proxy_file_path.push(format!("{}.proxy", name));
            create_proxy_file(proxy_file_path, proxy_url)?;
            run_reload(opt.mgmt_port)
        }
    }
}

fn run_log_command(port: u16, cmd: LogCommand, custom_level: Option<String>) -> Result<(), Error> {
    let request = match cmd {
        LogCommand::Debug => Request::SetLogLevel(LogLevel::DebugLevel),
        LogCommand::Trace => Request::SetLogLevel(LogLevel::TraceLevel),
        LogCommand::Custom => {
            let level = custom_level.unwrap(); // should be protected by clap 'requires'
            Request::SetLogLevel(LogLevel::CustomLevel(level))
        }
        LogCommand::Reset => Request::ResetLogLevel,
    };
    run_log(port, request)
}

#[cfg(test)]
mod tests {
    use super::*;

    use clap::ErrorKind;

    macro_rules! test_cli_ok {
        ($name:ident, $opts:expr) => {
            #[test]
            fn $name() {
                let names = Names::new();
                let app = create_cli_app(&names);
                assert!(app.get_matches_from_safe($opts).is_ok());
            }
        };
    }

    macro_rules! test_cli_error {
        ($name:ident, $opts:expr, $expected_error_kind:expr) => {
            #[test]
            fn $name() {
                let names = Names::new();
                let app = create_cli_app(&names);
                let res = app.get_matches_from_safe($opts);
                assert!(&res.is_err());
                assert_eq!(res.unwrap_err().kind, $expected_error_kind)
            }
        };
    }

    test_cli_ok! { accept_custom_log_level, vec!["duwop", "log", "custom", "level"] }
    test_cli_ok! {
        accept_link_cmd_with_three_positional_args,
        vec!["duwop", "link", "/some/path", "name"]
    }
    test_cli_ok! {
        accept_link_cmd_with_two_positional_args,
        vec!["duwop", "link", "name"]
    }

    test_cli_error! {
        log_custom_requires_level,
        vec!["duwop", "log", "custom"],
        ErrorKind::MissingRequiredArgument
    }
}
