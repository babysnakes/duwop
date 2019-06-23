use duwop::app_defaults::*;
use duwop::cli_helpers::*;
use duwop::client::*;

use std::path::PathBuf;

use dotenv;
use failure::Error;
use flexi_logger;
use log::debug;
use structopt::{self, StructOpt};
use url::Url;

// Fix verify global options
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
}

fn main() {
    dotenv::dotenv().ok();
    flexi_logger::Logger::with_env().start().unwrap();
    let app = Cli::from_args();
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
    }
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
