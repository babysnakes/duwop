use duwop::app_defaults::*;
use duwop::cli_helpers::*;
use duwop::client::*;
use duwop::setup;

use dotenv;
use failure::Error;
use flexi_logger::{self, style, DeferredNow, LevelFilter, LogSpecBuilder, Logger, Record};
use log::debug;
use std::io;
use std::path::PathBuf;
use structopt::{self, StructOpt};

#[derive(Debug, StructOpt)]
#[structopt(name = "duwopctl", author = "")]
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

    /// quiet mode - only report warnings and errors
    #[structopt(short = "q", global = true, conflicts_with = "verbose")]
    quiet: bool,

    #[structopt(subcommand)]
    command: CliSubCommand,
}

#[derive(Debug, StructOpt)]
enum CliSubCommand {
    /// Instruct the server to reload configuration.
    ///
    /// This is only required if you changed configuration manually. Every
    /// configuration command (e.g. link, proxy etc) will signal to duwop to
    /// reload configuration.
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

    /// Add directory to serve with web server.
    ///
    /// Add configuration to serve the specified target directory (or the
    /// current directory if none specified) with a web server accessible as
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

    /// Add local port to reverse proxy.
    ///
    /// Add a configuration to reverse proxy the specified port (on localhost)
    /// as http://<name>.test/. The name should not contain the '.test' domain.
    #[structopt(name = "proxy", author = "")]
    Proxy {
        /// The hostname to use for the proxy
        name: String,

        /// The port (on localhost) to point the proxy to
        port: u16,
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
    ///
    /// Also displays service mis-configurations.
    #[structopt(name = "list", author = "")]
    List,

    /// Run diagnostics (server, configuration, DNS, etc).
    ///
    /// Use after updates and when something seems wrong. Currently it doesn't
    /// really get smart state from the server, just the fact that it's running.
    #[structopt(name = "doctor", author = "")]
    Doctor,

    /// Generate shell completions for the provided shell.
    ///
    /// This command will generate a file with the matching name (for the
    /// requested shell) in the specified target directory (default to current
    /// directory).
    ///
    /// Completion scripts should be placed in spacial directories (depending on
    /// the shell). It is assumed that you know your shell requirements and
    /// where to save the completion script.
    #[structopt(name = "completion", author = "")]
    Completion {
        /// the shell to generate completion for
        #[structopt(name = "shell", raw(possible_values = r#"&["bash", "zsh", "fish"]"#))]
        shell: String,

        /// the directory to save the completion script to
        #[structopt(name = "target-dir", default_value = ".")]
        target_dir: String,
    },

    /// Setup duwop (for new installations or upgrades).
    ///
    /// Creates required directories, config files, agent configuration, etc.
    /// Should not modify existing files/configs except for agent configuration.
    /// If you want to run it safely without overwriting or restarting the agent
    /// use the --skip-agent option.
    #[structopt(name = "setup", author = "")]
    Setup {
        /// skip agent configuration (e.g. if you only want to create missing
        /// resolver file)
        #[structopt(long = "skip-agent")]
        skip_agent: bool,

        /// don't actually perform the setup, just print what will be done
        #[structopt(long = "dry-run")]
        dry_run: bool,

        /// disable TLS - service will ony serve through HTTP
        #[structopt(long = "disable-tls")]
        disable_tls: bool,
    },

    /// Remove system wide configurations (installed during setup).
    ///
    /// This will not remove logs and state directory as it is contained in it's
    /// own directory.
    #[structopt(name = "remove", author = "")]
    Remove {
        /// don't actually remove anything, just print what will be done
        #[structopt(long = "dry-run")]
        dry_run: bool,
    },
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
        Err(err) => {
            print_full_error(err);
            std::process::exit(1);
        }
    }
}

fn run(app: Cli) -> Result<(), Error> {
    debug!("running with options: {:#?}", app);
    let mgmt_port = app.mgmt_port.unwrap_or(MANAGEMENT_PORT);
    let state_dir = app.state_dir.unwrap_or_else(|| STATE_DIR.to_owned());
    let duwop_client = DuwopClient::new(mgmt_port, state_dir);
    match app.command {
        CliSubCommand::Reload => duwop_client.reload_server_configuration(),
        CliSubCommand::Log { command, level } => duwop_client.run_log_command(command, level),
        CliSubCommand::Link { name, source } => {
            duwop_client.create_static_file_configuration(name, source)
        }
        CliSubCommand::Proxy { name, port } => duwop_client.create_proxy_configuration(name, port),
        CliSubCommand::Delete { name } => duwop_client.delete_configuration(name),
        CliSubCommand::List => duwop_client.print_services(),
        CliSubCommand::Doctor => duwop_client.doctor(),
        CliSubCommand::Completion { shell, target_dir } => generate_completions(shell, target_dir),
        CliSubCommand::Setup {
            skip_agent,
            dry_run,
            disable_tls,
        } => setup::Setup::new(dry_run).run(skip_agent, disable_tls),
        CliSubCommand::Remove { dry_run } => setup::Setup::new(dry_run).remove(),
    }
}

fn format_log(w: &mut io::Write, _now: &mut DeferredNow, record: &Record) -> Result<(), io::Error> {
    let level = record.level();
    write!(w, "{}: {}", style(level, record.level()), &record.args(),)
}

fn generate_completions(shell: String, target_dir: String) -> Result<(), Error> {
    Cli::clap().gen_completions("duwopctl", shell.parse().unwrap(), &target_dir);
    Ok(())
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
