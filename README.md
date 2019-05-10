## DuWop - Serve Local Directories and Proxy Local Services

#### README oriented development :)

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

## Development environment setup

Copy `.env-sample` in the current directory to `.env` and edit to your liking.

## Contributors

* Big credit goes to [Emil Hernvall][emil] for his great [dnsguide][]. The
  entire DNS implementation is copied (with slight modifications) from his guide
  with his permission.

[pd]: https://github.com/puma/puma-dev
[emil]: https://github.com/EmilHernvall
[dnsguide]: https://github.com/EmilHernvall/dnsguide
