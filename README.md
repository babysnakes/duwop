## DuWop - Serve Local Directories and Proxy Local Services

### README oriented development :)

This project aims to perform the following tasks:

* Serve local directories as HTTP.
* Reverse proxy local running services.
* Reverse proxy local docker containers (by container name) provided they serve
  only one port (so there's no need to specify local ports when running `docker run...`).

It should have the following characteristics:

* Should provide local DNS server for `test` domain (should we make it
  configurable?) to point all addresses to localhost. All access should be by
  name, not IP.
* Bind to localhost on port 80 (and later 443 - see SSL bullet). We'll achieve
  this (without running as root) by running via launchd. Running on localhost to
  avoid having to deal with various security issues.
* Configuration for serving directories and reverse proxy is controlled by soft
  links and files containing the value in the first line. These files/links
  should be in a specific directory.
* Command line to setup / configure / control the various aspects of the
  service.
* Possibly SSL termination without having to hassle with invalid certificates.
  Check [puma dev][pd] for example how to perform this.

### What's implemented and setup instructions

The project is in early stages, installation is manual and requires some
technical skills and knowledge in building rust projects.

Currently we have (all should be considered beta at best):

* DNS server that returns `127.0.0.1` on all `.test` domain requests.
* Web server that listens on port 80 (with launchd) and can:
    * Serve local files (directory listing is not supported - we only serve
      `index.html` files inside directories)
    * Proxy local ports - basic proxy - no support for advance features like
      websockets. Not tested much.
* Reads configuration from _state directory_ with the option to reload from the
  client.
* Logs to rotating logs.
* Client app (`duwopctl`) that can perform the following tasks:
    * Tell server to reload configuration from disk.
    * Switch server log level in runtime.
    * Create static file serving configuration.
    * Create reverse proxy configuration.
    * Delete configuration.
    * List existing configurations.
    * Check status of server (currently only if it runs) and database (check for errors).

Setup instructions:

* Build the project (`cargo build --release`) and copy `target/release/duwop`
  and `target/release/duwopctl` to somewhere in your path.
* `mkdir -p $HOME/.duwop/logs`
* `mkdir $HOME/.duwop/state`
* Setup the state directory:
    * Run `duwopctl help link` for instructions on adding configuration for
      serving directories with static files.
    * Run `duwopctl help proxy` for instructions of adding reverse proxy
      configurations.
    * Use `dowopctl delete` / `duwopctl list` to delete / list configurations.
* Copy `extra/org.babysnakes.duwop.plist` to `~/Library/LaunchAgents/` and edit:
    * Configure `/path/to/duwop`.
    * *Do not* change the `127.0.0.1` hostname. This is a _major_ security issue
      as this project does not force authentication in any way! We rely on you
      listening only on ports unavailable from outside.
* Load the launchd configuration with `launchctl load /path/to/plist/file`.

Last thing to do is as root create `/etc/resolver` directory (if it doesn't
exist) and create a `test` file with the following content:

```
# duwop
nameserver 127.0.0.1
port 9053
```

Enjoy

### Development environment setup

* Copy `extra/env-sample` to `.env` in the repository root and edit to your
  liking.
* Create a `devdata` directory in the repository root - this will hold the
  development state directory. Use the various `duwopctl` commands (described
  above in the setup instructions) to setup the development state directory,
  _however_, use the undocumented `--state-dir` option (or `DUWOP_APP_STATE_DIR`
  environment variable) to make sure your editing the development state
  directory. If you have setup your `.env` file correctly (previous bullet) and
  you are running from within the repository root you don't have to do anything,
  the development `devdata` directory is enabbled automatically.
* Do not use `--log-to-file` option as the log directory is hard-coded.

### Contributors

* Big credit goes to [Emil Hernvall][emil] for his great [dnsguide][]. The
  entire DNS implementation is copied (with slight modifications) from his guide
  with his permission.
* The [basic-http-server][bhttp] project. The base of the static files serving
  code is copied from this project.


[pd]: https://github.com/puma/puma-dev
[emil]: https://github.com/EmilHernvall
[dnsguide]: https://github.com/EmilHernvall/dnsguide
[bhttp]: https://github.com/brson/basic-http-server
