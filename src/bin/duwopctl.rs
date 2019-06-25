use duwop::app_defaults::*;
use duwop::cli_helpers::*;
use duwop::client::*;

use dotenv;
use failure::Error;
use flexi_logger::{self, style, DeferredNow, LevelFilter, LogSpecBuilder, Logger, Record};
use log::debug;
use std::io;
use std::path::PathBuf;
use structopt::{self, StructOpt};
use url::Url;

#[derive(Debug, StructOpt)]
#[structopt(name = "duwopctl", author = "", raw(version = "VERSION"))]
/// Configure/Manage duwop service.
struct Cli {
    /// alternative management port
    #[structopt(
        long = "mgmt-port",
        value_name = "PORT",
        global = true,
        env = "DUWOP_MANAGEMENT_PORT"
    )]
    mgmt_port: Option<u16>,

    #[structopt(
        long = "state-dir",
        hidden = true,
        global = true,
        env = "DUWOP_APP_STATE_DIR"
    )]
    state_dir: Option<PathBuf>,

    /// verbose logging (multiple times for extra verbosity)
    #[structopt(name = "verbose", short = "v", global = true, parse(from_occurrences))]
    verbose: u8,

    /// only log warnings and errors
    #[structopt(short = "q", global = true, conflicts_with = "verbose")]
    quiet: bool,

    #[structopt(subcommand)]
    command: CliSubCommand,
}

#[derive(Debug, StructOpt)]
enum CliSubCommand {
    /// Reload duwop configuration from disk.
    #[structopt(name = "reload", author = "")]
    Reload,

    /// Change log level on the duwop server during runtime.
    ///
    /// This command lets you switch log level on the duwop service in runtime.
    /// It will reset itself once it restarted. Use the 'reset' argument to
    /// reset to default log level.
    #[structopt(name = "log", author = "")]
    Log {
        /// Log level to switch the service to (reset to return to default)
        #[structopt(
            name = "log_command",
            case_insensitive = true,
            raw(possible_values = "&LogCommand::variants()")
        )]
        command: LogCommand,

        /// custom log level (e.g. trace,tokio:warn). valid only if log-level is
        /// 'custom'
        #[structopt(raw(required_if = r#""log_command", "custom""#))]
        level: Option<String>,
    },

    /// Serve directory with web server.
    ///
    /// This command will serve the specified target directory (or the current
    /// directory if none specified) with a web server accessible as
    /// http://<name>.test/. The name should not include the '.test' domain.
    #[structopt(name = "link", author = "")]
    Link {
        /// The hostname to serve the directory as
        #[structopt(name = "name")]
        name: String,

        /// The directory to serve, if omitted the current directory is used
        #[structopt(name = "source_dir")]
        source: Option<PathBuf>,
    },

    /// Reverse proxy a URL
    ///
    /// This command will serve the specified target directory (or the current
    /// directory if none specified) with a web server accessible as
    /// http://<name>.test/. The name should not include the '.test' domain.
    #[structopt(name = "proxy", author = "")]
    Proxy {
        /// The hostname to use to proxy the URL
        name: String,

        /// The URL to reverse proxy to, you will be prompted for it if not
        /// specified
        url: Option<Url>,
    },

    /// Deletes configuration (serve directory or reverse proxy)
    ///
    /// Use this command to delete the service by name (wether it's a directory
    /// or reverse proxy). Run 'duwop list' to see available services.
    #[structopt(name = "delete", author = "")]
    Delete {
        /// The name of the service to delete
        name: String,
    },

    /// List available services.
    #[structopt(name = "list", author = "")]
    List,
}

fn main() {
    dotenv::dotenv().ok();
    let app = Cli::from_args();
    let mut builder = LogSpecBuilder::new();
    if app.quiet {
        builder.module("duwop", LevelFilter::Warn);
    } else {
        match app.verbose {
            0 => builder.module("duwop", LevelFilter::Info),
            1 => builder.module("duwop", LevelFilter::Debug),
            2 => builder.module("duwop", LevelFilter::Trace),
            _ => builder.default(LevelFilter::Trace),
        };
    }
    Logger::with(builder.build())
        .format_for_stderr(format_log)
        .start()
        .unwrap();
    match run(app) {
        Ok(_) => {}
        Err(err) => print_full_error(err),
    }
}

fn run(app: Cli) -> Result<(), Error> {
    debug!("running with options: {:#?}", app);
    let mgmt_port = app.mgmt_port.unwrap_or(MANAGEMENT_PORT);
    let state_dir = app.state_dir.unwrap_or_else(state_dir);
    let duwop_client = DuwopClient::new(mgmt_port, state_dir);
    match app.command {
        CliSubCommand::Reload => duwop_client.reload_server_configuration(),
        CliSubCommand::Log { command, level } => duwop_client.run_log_command(command, level),
        CliSubCommand::Link { name, source } => {
            duwop_client.create_static_file_configuration(name, source)
        }
        CliSubCommand::Proxy { name, url } => duwop_client.create_proxy_configuration(name, url),
        CliSubCommand::Delete { name } => duwop_client.delete_configuration(name),
        CliSubCommand::List => duwop_client.print_services(),
    }
}

fn format_log(w: &mut io::Write, now: &mut DeferredNow, record: &Record) -> Result<(), io::Error> {
    let level = record.level();
    write!(
        w,
        "{}[{}] {}",
        style(level, record.level()),
        now.now().format("%H:%M:%S"),
        &record.args(),
    )
}

#[cfg(test)]
mod tests {
    use super::*;

    use structopt::clap::ErrorKind;

    macro_rules! test_cli_ok {
        ($name:ident, $opts:expr) => {
            #[test]
            fn $name() {
                let app = Cli::clap();
                assert!(app.get_matches_from_safe($opts).is_ok());
            }
        };
    }

    macro_rules! test_cli_error {
        ($name:ident, $opts:expr, $expected_error_kind:expr) => {
            #[test]
            fn $name() {
                let app = Cli::clap();
                let res = app.get_matches_from_safe($opts);
                assert!(&res.is_err());
                assert_eq!(res.unwrap_err().kind, $expected_error_kind)
            }
        };
    }

    test_cli_ok! { accept_custom_log_level, vec!["duwop", "log", "custom", "level"] }
    test_cli_ok! {
        accept_link_cmd_with_three_positional_args,
        vec!["duwopctl", "link", "/some/path", "name"]
    }
    test_cli_ok! {
        accept_link_cmd_with_two_positional_args,
        vec!["duwopctl", "link", "name"]
    }

    test_cli_error! {
        log_custom_requires_level,
        vec!["duwopctl", "log", "custom"],
        ErrorKind::MissingRequiredArgument
    }

    test_cli_error! {
        verbosity_and_quiet_do_not_go_together,
        vec!["duwopctl", "-v", "-q", "list"],
        ErrorKind::ArgumentConflict
    }
}
