use super::app_defaults::*;

use std::io::Write;
use std::path::{Path, PathBuf};
use std::process::Command;

use dirs;
use failure::{format_err, Error, ResultExt};
use log::{debug, info};
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
}

impl Setup {
    pub fn new(dry_run: bool) -> Self {
        let home_dir = dirs::home_dir().expect("couldn't infer home directory!");
        let mut state_dir = home_dir.clone();
        state_dir.push(STATE_DIR_RELATIVE);
        let mut log_dir = home_dir.clone();
        log_dir.push(LOG_DIR);
        let mut launchd_agents_dir = home_dir.clone();
        launchd_agents_dir.push(&LAUNCH_AGENTS_DIR);
        let launchd_filename = format!("{}.plist", &AGENT_NAME);
        let mut agent_file = launchd_agents_dir.clone();
        agent_file.push(&launchd_filename);

        Setup {
            dry_run,
            state_dir,
            log_dir,
            launchd_agents_dir,
            agent_file,
            resolver_directory: RESOLVER_DIR.to_owned(),
            resolver_file: format!("{}/{}", &RESOLVER_DIR, RESOLVER_FILE),
        }
    }

    pub fn run(&self, skip_agent: bool) -> SetupResult {
        &self.create_duwop_dirs()?;
        if skip_agent {
            info!("skipping agent setup");
        } else {
            self.install_gent()?;
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

    fn create_duwop_dirs(&self) -> SetupResult {
        if self.dry_run {
            info!("would create {:?}", self.state_dir.as_os_str());
            info!("would create {:?}", self.log_dir.as_os_str());
        } else {
            info!("creating required directories");
            std::fs::create_dir_all(&self.state_dir).context("creating state directory")?;
            std::fs::create_dir_all(&self.log_dir).context("creating logs directory")?;
        }
        Ok(())
    }

    fn install_gent(&self) -> SetupResult {
        info!("installing launchd agent");
        let duwop_exe = find_duwop_exe()?;
        let launchd_text = format!(r#"<?xml version="1.0" encoding="UTF-8"?>
<!DOCTYPE plist PUBLIC "-//Apple//DTD PLIST 1.0//EN" "http://www.apple.com/DTDs/PropertyList-1.0.dtd">
<plist version="1.0">
  <dict>
    <key>Label</key>
    <string>{agent}</string>
    <key>ProgramArguments</key>
    <array>
      <string>{path}</string>
      <string>--launchd</string>
    </array>
    <key>Sockets</key>
    <dict>
      <key>DuwopSocket</key>
      <dict>
        <key>SockNodeName</key>
        <string>127.0.0.1</string>
        <key>SockServiceName</key>
        <string>80</string>
      </dict>
    </dict>
    <key>RunAtLoad</key>
    <true/>
  </dict>
</plist>
"#, 
            agent=AGENT_NAME,
            path=duwop_exe,
        );
        let agent_file = &self.agent_file.to_str().unwrap();
        if self.dry_run {
            info!("would create directory: {:?}", self.launchd_agents_dir);
        } else {
            std::fs::create_dir_all(&self.launchd_agents_dir)?;
        }
        if self.agent_file.exists() {
            info!("launchd file exists, will unload and overwrite");
            // we can safely ignore this command
            let _ = run_command(vec!["launchctl", "unload", agent_file], "", self.dry_run);
        }
        if self.dry_run {
            info!("would create {}", agent_file);
        } else {
            let mut f = std::fs::File::create(&self.agent_file)?;
            f.write_all(launchd_text.as_bytes())?;
        }
        run_command(
            vec!["launchctl", "load", agent_file],
            "failed loading agent",
            self.dry_run,
        )
    }

    fn install_resolve_file(&self) -> SetupResult {
        info!("setting up resolver");
        let resolver_file = Path::new(&self.resolver_file);
        if resolver_file.exists() {
            info!(
                "resolver file ({}) already exists (possibly from previous version?). skipping.",
                &self.resolver_file
            );
            return Ok(());
        }
        let content = format!(
            "# Generated by duwop\nnameserver 127.0.0.1\nport {}\n",
            &DNS_PORT
        );
        debug!("creating resolver file");
        println!(
            "{} you might be prompted for your (sudo) password for creating {}",
            Paint::green("Note:"),
            &self.resolver_file,
        );
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
        info!("would run: {:#?}", &command);
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
    // in dry run mode this should not return error becaues it doesn't try to
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
