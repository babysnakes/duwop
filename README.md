## DuWop - Serve Local Directories and Proxy Local Services

> *Note:* This project is currently *Mac Only*. I might add linux later (with
required manual 3rd-party installation) but there's still time.

This project aims to perform the following tasks:

* Serve local directories as HTTP.
* Reverse proxy local running services.
* Reverse proxy local docker containers (by container name) provided they serve
  only one port (so there's no need to specify local ports when running `docker
  run...`).
* Access all the described above through host-names in the *test* domain (e.g.
  *mydevproject.test*).
* Optionally have SSL termination with locally trusted certificate for all the
  above services
* Web access should be through default ports (80\443). You don't have to specify
  custom high ports (e.g. localhost:3000).

To control the service you have a command-line utility (`duwopctl`). It helps
adding/removing services, configure the system or remove configurations, service
diagnostics and more.

Check [here](#Project-status) for project status.

### Setup Instructions

The project is in early stages so the setup script might have bugs, Please
report setup bugs.

*Note:* You have to be admin on your computer in order to run setup.

#### From binary release
* Obtain the latest binary release from the repository's releases page.
* Open the archive (`tar xzf duwop-bin-...` or double click it).
* Step into the generated `duwop` directory and run: `./install.sh`.
* Follow the on-screen instructions.

#### From source
*Note:* you have to have rust compiler installed in order to perform this step.

* From the repository root run: `./release.sh prepare`.
* Step into `target/duwop` and run: `./install.sh`.
* Follow the on-screen instructions.

Enjoy

### Project status

>Note that even the implemented features are of beta quality, Please report bugs.

* Serving directories:
  * Directory listing are not supported. If no `index.html` file in the
    directory 404 is returned (low priority).
* Reverse proxy:
  * Only basic reverse proxy is supported. No support for streams, upgrades etc
    (high priority).
* Docker serving is not yet implemented. It's possible to implement it manually
  in terms of reverse proxy.

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
