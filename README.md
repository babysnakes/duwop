## DuWop - Serve Local Directories and Proxy Local Services

> *Note:* This project is currently *Mac Only*. I might add linux later (with
required manual 3rd-party installation) but there's still time.

This project aims to perform the following tasks:

* Serve local directories as HTTP.
* Reverse proxy local web servers running on other ports (e.g. webpack, api dev
  server etc).
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

The project is in beta state. Please report any setup (and other) bugs.

*Note:* You have to be admin on your computer in order to run setup.

#### From binary release
* Obtain the [latest binary][latest] release from the repository's releases
  page.
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

>Note that even the implemented features are of beta quality, Please report
>bugs.

**Missing Features**

* Docker serving is not yet implemented. It's possible to implement it manually
  in terms of reverse proxy.

**Planned Enhancements**:

* Serving directories:
  * Directory listing are not supported. If no `index.html` file in the
    directory 404 is returned - low priority.
* Reverse proxy:
  * Possibly allow to proxy web servers that are running on other internal ports
    (e.g. in VMware etc) - very low priority.
* SSL Support:
  * We are generating certificates only for the configured services and their
    subdomains (this is the only way it can work as far as I know - details are
    in the [certificates page][certs]). Because of that, newly created services
    are not supported until the service is reloaded (reloading instructions are
    [here][reloading]). This is high priority fix.
  * Trusting self signed certificates is a moving target. Currently configuring
    this trust is a manual step (documented [here][trust-cert]). We're trying to
    find a way to automate this.

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
  the development `devdata` directory is enabled automatically.
* Do not use `--log-to-file` option as the log directory is hard-coded.

### Contributors

* Big credit goes to [Emil Hernvall][emil] for his great [dnsguide][]. The
  entire DNS implementation is copied (with slight modifications) from his guide
  with his permission.
* Another big credit goes to [Klaus Purer][klaus1] for his [rustnish][] project. My proxy implementation is heavily based on this project.
* The [basic-http-server][bhttp] project. The base of the static files serving
  code is copied from this project.

### Breaking Changes

#### 0.3.0-beta1

* Proxy configuration has changed from `proxy:http://hostname:port/` to
  `proxy:hostname:post`. No auto conversion.

#### 0.4.0-beta1
* Agent name (and file name) changed from `org.babysnakes.duwop...` to
  `io.duwop...`. In the highly unlikely event that you have installed this
  before *0.4.0-beta1*, you'll have to manually stop and delete agent file for
  the new version to run:
```bash
# if you still have the old version installed run:
duwopctl remove
# otherwise perform it manually
launchctl unload ~/Library/LaunchAgents/org.babysnakes.duwop.plist
rm ~/Library/LaunchAgents/org.babysnakes.duwop.plist
```

[latest]: https://github.com/babysnakes/duwop/releases/latest
[trust-cert]: https://git.io/fjd6Z
[certs]: https://github.com/babysnakes/duwop/wiki/Certificates
[reloading]: https://github.com/babysnakes/duwop/wiki/Certificates#reloading-the-service-to-generate-certificates-for-new-services
[pd]: https://github.com/puma/puma-dev
[emil]: https://github.com/EmilHernvall
[dnsguide]: https://github.com/EmilHernvall/dnsguide
[klaus1]: https://klau.si
[bhttp]: https://github.com/brson/basic-http-server
[rustnish]: https://github.com/klausi/rustnish