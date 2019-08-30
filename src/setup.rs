use super::app_defaults::*;
use super::ssl;

use std::io::Write;
use std::path::{Path, PathBuf};
use std::process::Command;

use failure::{format_err, Error, ResultExt};
use log::{debug, trace};
use yansi::Paint;

type SetupResult = Result<(), Error>;

pub struct Setup {
    dry_run: bool,
    state_dir: PathBuf,
    log_dir: PathBuf,
    launchd_agents_dir: PathBuf,
    agent_file: PathBuf,
    resolver_directory: String,
    resolver_file: String,
    ca_dir: PathBuf,
    ca_key_file: PathBuf,
    ca_cert_file: PathBuf,
}

enum IoOperation {
    /// Create directory with all required parent dirs.
    MkdirAll(PathBuf),
    /// Delete a path (like `unlink`).
    RemoveFile(PathBuf),
    /// Create file with provided bytes.
    WriteFile(PathBuf, Vec<u8>),
    /// Create file with provided string.
    WriteStringToFile(PathBuf, String),
}

impl Setup {
    pub fn new(dry_run: bool) -> Self {
        Setup {
            dry_run,
            state_dir: STATE_DIR.to_owned(),
            log_dir: LOG_DIR.to_owned(),
            launchd_agents_dir: USER_LAUNCHD_DIR.to_owned(),
            agent_file: LAUNCHD_AGENT_FILE.to_owned(),
            resolver_directory: RESOLVER_DIR.to_owned(),
            resolver_file: format!("{}{}", &RESOLVER_DIR, RESOLVER_FILE),
            ca_dir: CA_DIR.to_owned(),
            ca_key_file: CA_KEY.to_owned(),
            ca_cert_file: CA_CERT.to_owned(),
        }
    }

    pub fn run(&self, skip_agent: bool, disable_tls: bool) -> SetupResult {
        self.create_duwop_dirs()?;
        if !disable_tls {
            self.install_ca_cert()?;
        }
        if skip_agent {
            info("skipping agent setup");
        } else {
            self.install_agent(disable_tls)?;
        }
        self.install_resolve_file()?;
        let bullet = format!("{} ", Paint::green("*"));
        let wrapper = textwrap::Wrapper::with_termwidth()
            .initial_indent(&bullet)
            .subsequent_indent("  ");
        println!("\n{}", Paint::green("==============="));
        println!("\nSetup completed\n");
        println!(
            "{}",
            wrapper.fill("run 'duwopctl doctor' to check service, setup and db health")
        );
        if !disable_tls {
            println!(
                "{}",
                wrapper.fill(
                    "more info about working with self-signed certificates: https://git.io/fjd6Z"
                )
            );
        }
        println!(
            "{}",
            wrapper.fill("use 'duwopctl completion ...' to generate shell completion")
        );
        println!(
            "{}",
            wrapper.fill("use 'duwopctl link | proxy ...' to add services")
        );
        println!(
            "{}",
            wrapper.fill("run 'duwopctl help' for available commands")
        );
        println!("\nEnjoy :)");
        Ok(())
    }

    pub fn remove(&self) -> SetupResult {
        info("Removing duwop configurations");
        self.remove_agent()?;
        self.remove_resolver_file()?;

        println!("\n{}", Paint::green("======================"));
        println!("\nConfigurations removed\n");
        // TODO: print uninstall instruction.
        Ok(())
    }

    fn create_duwop_dirs(&self) -> SetupResult {
        info("creating required directories under $HOME/.duwop");
        let create_state_dir = IoOperation::MkdirAll(self.state_dir.to_owned());
        let create_log_dir = IoOperation::MkdirAll(self.log_dir.to_owned());
        create_state_dir
            .perform(self.dry_run)
            .context("creating state directory")?;
        create_log_dir
            .perform(self.dry_run)
            .context("creating log directory")?;
        Ok(())
    }

    fn install_agent(&self, disable_tls: bool) -> SetupResult {
        info("installing launchd agent");
        let duwop_exe = find_duwop_exe()?;
        let launchd_text = generate_launchd_template(&duwop_exe, disable_tls)
            .context("generating launchd agent template")?;
        let agent_file = &self.agent_file.to_str().unwrap();
        let create_agents_dir = IoOperation::MkdirAll(self.launchd_agents_dir.to_owned());
        create_agents_dir
            .perform(self.dry_run)
            .context("creating launchd agents directory")?;
        if self.agent_file.exists() {
            info("launchd file exists, will unload and overwrite");
            // we can safely ignore this command
            let _ = run_command(vec!["launchctl", "unload", agent_file], "", self.dry_run);
        }
        let create_agent_file =
            IoOperation::WriteStringToFile(self.agent_file.clone(), launchd_text);
        create_agent_file
            .perform(self.dry_run)
            .context("creating launchd agent file")?;
        run_command(
            vec!["launchctl", "load", agent_file],
            "failed loading agent",
            self.dry_run,
        )
    }

    fn install_resolve_file(&self) -> SetupResult {
        info("setting up resolver");
        let resolver_file = Path::new(&self.resolver_file);
        if resolver_file.exists() {
            info(&format!(
                "resolver file ({}) already exists (possibly from previous version?). skipping.",
                &self.resolver_file
            ));
            return Ok(());
        }
        let content = format!(
            "# Generated by duwop\nnameserver 127.0.0.1\nport {}\n",
            &DNS_PORT
        );
        debug!("creating resolver file");
        tell(&format!(
            "{} you might be prompted for your (sudo) password for creating {}",
            Paint::new("Note:").bold(),
            &self.resolver_file,
        ));
        run_command(
            vec!["sudo", "mkdir", "-p", &self.resolver_directory],
            "error creating resolver directory",
            self.dry_run,
        )?;
        run_command(
            vec!["sudo", "chmod", "o+rx", &self.resolver_directory],
            "error setting permissions on resolver directory",
            self.dry_run,
        )?;
        let resolver_cmd = format!(r#"echo "{}" > {}"#, &content, &self.resolver_file);
        run_command(
            vec!["sudo", "sh", "-c", &resolver_cmd],
            "error creating resolver file",
            self.dry_run,
        )?;
        run_command(
            vec!["sudo", "chmod", "644", &self.resolver_file],
            "error setting permissions on resolver file",
            self.dry_run,
        )?;
        Ok(())
    }

    fn install_ca_cert(&self) -> SetupResult {
        if !self.ca_required().context("checking for valid CA")? {
            info("valid CA already exists");
            return Ok(());
        }

        info("creating a new SSL root certificate");
        debug!("creating new Ca private key and certificate");
        let (cert, key) = ssl::mk_ca_cert()?;
        let required_dir = IoOperation::MkdirAll(self.ca_dir.to_owned());
        let save_key =
            IoOperation::WriteFile(self.ca_key_file.to_owned(), key.private_key_to_pem_pkcs8()?);
        let save_cert = IoOperation::WriteFile(self.ca_cert_file.to_owned(), cert.to_pem()?);
        required_dir.perform(self.dry_run)?;
        save_key
            .perform(self.dry_run)
            .context("saving CA private key")?;
        save_cert
            .perform(self.dry_run)
            .context("saving CA certificate")?;
        info("Adding the newly created certificate to your keychain - so your browsers will trust it");
        tell("You might be prompted to approve installing the created CA as a trusted certificate in your keychain");
        run_command(
            vec![
                "security",
                "add-trusted-cert",
                "-k",
                "login.keychain-db",
                self.ca_cert_file.to_str().unwrap(),
            ],
            "installing certificate in keychain",
            self.dry_run,
        )?;
        Ok(())
    }

    fn remove_agent(&self) -> SetupResult {
        let agent_file = &self.agent_file.to_str().unwrap();
        info("removing duwop agent configuration");
        if self.dry_run {
            tell(&format!(
                "{} you will see the 'unload' command running twice, that's intentional",
                Paint::new("Note:").bold()
            ));
        }
        // We are running twice so in debug mode we'll see at least one output
        // about agent that is not loaded.
        for _ in [1, 2].iter() {
            // we can let it fail
            let _ = run_command(
                vec!["launchctl", "unload", agent_file],
                "error unloading duwop service",
                self.dry_run,
            );
        }
        let remove_file = IoOperation::RemoveFile(self.agent_file.clone());
        remove_file
            .perform(self.dry_run)
            .context("deleting agent file")?;
        Ok(())
    }

    fn remove_resolver_file(&self) -> SetupResult {
        info("removing resolver file");
        tell(&format!(
            "{} you might be prompted for your (sudo) password for deleting {}",
            Paint::new("Note:").bold(),
            &self.resolver_file,
        ));
        run_command(
            vec!["sudo", "rm", &self.resolver_file],
            "error deleting resolver file",
            self.dry_run,
        )
    }

    /// Checks wether we need to setup new CA. Should return an error _only_ if
    /// there's an err processing the certificate, not if it's not valid.
    fn ca_required(&self) -> Result<bool, Error> {
        match ssl::load_ca_cert(self.ca_key_file.to_owned(), self.ca_cert_file.to_owned()) {
            Ok((cert, _)) => {
                if ssl::validate_ca(cert, CA_EXPIRED_GRACE)
                    .context("validating found certificate")?
                {
                    debug!("found valid CA certificate");
                    Ok(false)
                } else {
                    warn("Your TLS certificate is out of date or nearing it. We will replace it.");
                    Ok(true)
                }
            }
            Err(e) => {
                debug!("failed loading CA");
                trace!("error was: {}", e);
                Ok(true)
            }
        }
    }
}

impl IoOperation {
    fn perform(&self, dry_run: bool) -> Result<(), std::io::Error> {
        match self {
            IoOperation::RemoveFile(file) => {
                if dry_run {
                    tell(&format!("Would delete {:?}", &file));
                    Ok(())
                } else {
                    debug!("deleting file {:?}", &file);
                    std::fs::remove_file(&file)
                }
            }
            IoOperation::MkdirAll(dir) => {
                if dry_run {
                    tell(&format!("would create directory {:?}", dir));
                    Ok(())
                } else {
                    debug!("creating directory {:?}", &dir);
                    std::fs::create_dir_all(&dir)
                }
            }
            IoOperation::WriteFile(file, bytes) => {
                if dry_run {
                    tell(&format!("would create file: {:?}", &file));
                    Ok(())
                } else {
                    debug!("creating file: {:?}", &file);
                    let mut f = std::fs::File::create(&file)?;
                    f.write(&bytes[..]).map(|_| ())
                }
            }
            IoOperation::WriteStringToFile(file, content) => {
                if dry_run {
                    tell(&format!("would create file: {:?}", &file));
                    Ok(())
                } else {
                    debug!("creating file: {:?}", &file);
                    let mut f = std::fs::File::create(&file)?;
                    f.write_all(content.as_bytes())
                }
            }
        }
    }
}

/// A helper for running shell commands. It handles debug logging, dry run and
/// error messages.
///
/// * The first item of *cmd* is the command to run. The rest are the
///   arguments.
/// * The *error_msg* is used as the prefix for showing the stderr of the
///   command (`"error_msg: stderr"`).
fn run_command(cmd: Vec<&str>, error_msg: &str, dry_run: bool) -> SetupResult {
    if cmd.is_empty() {
        return Err(format_err!("empty commands are not allowed"));
    }

    let mut command = Command::new(cmd.first().unwrap());
    command.args(&cmd[1..]);
    if dry_run {
        tell(&format!("would run: {:#?}", &command));
        Ok(())
    } else {
        debug!("running: {:#?}", &command);
        match command.output() {
            Ok(output) => {
                debug!("result: {:#?}", output);
                if output.status.success() {
                    Ok(())
                } else {
                    Err(format_err!(
                        "{}: {}",
                        error_msg,
                        std::str::from_utf8(&output.stderr).unwrap()
                    ))
                }
            }
            Err(err) => Err(format_err!("error invoking ({:?}), {}", &command, err)),
        }
    }
}

/// Returns the path for the _duwop_ executable.
fn find_duwop_exe() -> Result<String, Error> {
    let current_exe = std::env::current_exe().context("finding current executable")?;
    // The executable path is a symlink
    let original = current_exe
        .canonicalize()
        .context("finding duwopctl link source")?;
    let mut duwop_path = original.parent().unwrap().to_path_buf(); // only fails on "/".
    duwop_path.push("duwop");
    if duwop_path.exists() {
        Ok(duwop_path
            .to_str()
            .expect("couldn't convert duwop_path to string")
            .to_owned())
    } else {
        Err(format_err!("duwop path ({:?}) does not exist", &duwop_path))
    }
}

fn generate_launchd_template(exe_path: &str, disable_tls: bool) -> Result<String, Error> {
    use serde::Serialize;
    use tinytemplate::TinyTemplate;
    use trim_margin::MarginTrimmable;

    #[derive(Serialize)]
    struct Context<'a> {
        agent_name: &'a str,
        executable: &'a str,
        socket_name: &'a str,
        tls_socket_name: &'a str,
        enable_tls: bool,
    }
    let context = Context {
        agent_name: AGENT_NAME,
        executable: exe_path,
        socket_name: LAUNCHD_SOCKET,
        tls_socket_name: LAUNCHD_TLS_SOCKET,
        // it's kind of confusing but I prefer for the default to be enabled and
        // pass argument for disabling. On the template though it's more clear
        // if we enable it :)
        enable_tls: !disable_tls,
    };
    let launchd_agent_template = r#"
        |<?xml version="1.0" encoding="UTF-8"?>
        |<!DOCTYPE plist PUBLIC "-//Apple//DTD PLIST 1.0//EN" "http://www.apple.com/DTDs/PropertyList-1.0.dtd">
        |<plist version="1.0">
        |<dict>
        |    <key>Label</key>
        |    <string>{agent_name}</string>
        |    <key>ProgramArguments</key>
        |    <array>
        |    <string>{executable}</string>
        |    <string>--launchd</string>{{if enable_tls }}{{ else }}
        |    <string>--disable-tls</string>{{ endif }}
        |    </array>
        |    <key>Sockets</key>
        |    <dict>
        |    <key>{socket_name}</key>
        |    <dict>
        |        <key>SockNodeName</key>
        |        <string>127.0.0.1</string>
        |        <key>SockServiceName</key>
        |        <string>80</string>
        |    </dict>{{ if enable_tls }}
        |    <key>{tls_socket_name}</key>
        |    <dict>
        |        <key>SockNodeName</key>
        |        <string>127.0.0.1</string>
        |        <key>SockServiceName</key>
        |        <string>443</string>
        |    </dict>{{ endif }}
        |    </dict>
        |    <key>RunAtLoad</key>
        |    <true/>
        |</dict>
        |</plist>
    "#.trim_margin().ok_or_else(|| format_err!("error rendering launchd template"))?;
    let mut tt = TinyTemplate::new();
    tt.add_template("main", &launchd_agent_template)?;
    tt.render("main", &context).map_err(Error::from)
}

fn info(text: &str) {
    print(Paint::green("->"), text);
}

fn tell(text: &str) {
    print(Paint::yellow("->"), text);
}

fn warn(text: &str) {
    print(Paint::red("->"), text);
}

fn print(arrow: Paint<&str>, text: &str) {
    let initial = format!("{} ", arrow);
    let wrapper = textwrap::Wrapper::with_termwidth()
        .initial_indent(&initial)
        .subsequent_indent("   ");
    println!("{}", wrapper.fill(text));
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn run_command_errors_if_command_is_empty() {
        let result = run_command(vec![], "message", true);
        if let Err(err) = result {
            assert!(err.to_string().contains("empty commands"));
        } else {
            panic!("empty command should return error");
        }
    }

    #[test]
    fn run_command_does_not_run_the_command_in_dry_run_mode() {
        // in dry run mode this should not return error because it doesn't try to
        // run the invalid command.
        let result = run_command(vec!["/no/such/command"], "error", true);
        assert!(result.is_ok());
    }

    #[test]
    fn run_command_errors_on_invocation_error() {
        let result = run_command(vec!["/no/such/command"], "error", false);
        if let Err(err) = result {
            assert!(err.to_string().contains("error invoking"));
            assert!(err.to_string().contains("/no/such/command"));
            assert!(err.to_string().contains("No such file"));
        } else {
            panic!("invalid command should return error");
        }
    }

    #[test]
    fn run_command_errors_if_command_fails() {
        let result = run_command(vec!["ls", "/no/such/directory"], "error_message", false);
        println!("{:#?}", result);
        if let Err(err) = result {
            assert!(err.to_string().contains("error_message"));
        } else {
            panic!("such command should not succeed");
        }
    }

    #[test]
    fn test_template_with_tls() {
        let result = generate_launchd_template("/some/path", false).unwrap();
        // println!("with tls: {}", result);
        assert!(result.contains(AGENT_NAME), "failed agent name");
        assert!(
            result.contains(LAUNCHD_TLS_SOCKET),
            "should contain tls socket name"
        );
        assert!(
            !result.contains("--disable-tls"),
            "should not contain disable tls flag"
        );
    }

    #[test]
    fn test_template_without_tls() {
        let result = generate_launchd_template("/some/path", true).unwrap();
        // println!("without tls: {}", result);
        assert!(
            !result.contains(LAUNCHD_TLS_SOCKET),
            "should not contains tls socket deceleration"
        );
        assert!(
            result.contains("--disable-tls"),
            "should contain disable-tls flag"
        );
    }
}
