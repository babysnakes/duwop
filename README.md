## DuWop - Serve Local Directories and Proxy Local Services

### README oriented development :)

This project aims to perform the following tasks:

* Serve local directories as HTTP (configure via CLI or web interface).
* Reverse proxy local running services.
* Possibly easy configuration via web interface for reverse proxy docker
  containers.

It should have the following characteristics:

* Should provide local DNS server for `test` domain (should we make it
  configurable?) to point all addresses to localhost. All access should be by
  name, not IP.
* Bind to localhost on port 80 (and later 443 - see SSL bullet). We'll achieve
  this (without running as root) by running via launchd. Running on localhost to
  avoid having to deal with various security issues.
* Web interface to configure directories/ports/docker containers.
* Possibly SSL termination without having to hassle with invalid certificates.
  Check [puma dev][pd] for example how to perform this.

### What's implemented and setup instructions

The project is in early stages, installation is manual and requires some
technical skills and knowledge in building rust projects.

Currently we have (all should be considered beta at best):

* DNS server that returns `127.0.0.1` on all `.test` domain requests.
* Web server that listens on port 80 and can:
    * Serve local files (directories are not supported - we only serve
      `index.html` files inside directories)
    * Proxy local ports - basic proxy - no support for advance features like
      websockets. Not tested much.
* Can only read json configurations file. Any config modifications should be
  performed manually and the service should be restarted with `launchctl`.

Setup instructions:

* Build the project (`cargo build --release`) and copy `target/release/duwop` to
  somewhere in your path.
* `mkdir $HOME/.duwop`
* Copy `extra/devstate-sample.json` to `$HOME/.duwop/state.json` and follow the
  examples in the file to add real directories/ports to serve/proxy. Note that
  the keys indicate hostname in the `.test` domain (so `example` key means you
  have to access `example.test`).
* Copy `extra/org.babysnakes.duwop.plist` to `~/Library/LaunchAgents/` and edit:
    * Configure `/path/to/duwop`.
    * You can change the `RUST_LOG` value to something else or completely remove
      these two lines. It's only for debugging.
    * Set the `/path/to/stderr/file` and `/path/to/stdout/file` to a valid
      directory (optionally inside of `~/.duwop/` - I think you have to put
      explicit paths, there's no shell expansion in launchd files)
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

* Copy `extra/.env-sample` to `.env` in the current directory and edit to your
  liking.
* Copy `extra/devstate-sample.json` to `devstate.json` (or any other name and
  update the `.env` file) and edit keys with paths for static serving.

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
