# DuWop - Serve Local Directories and Proxy Local Services

> *NOTE:* This is currently re-written from scratch.

This project aims to perform the following tasks:

* Serve local directories as HTTP.
* Reverse proxy local web servers running on other ports (e.g. webpack, api dev
  server etc). We might add the option to launch a command when accessing a web
  address (e.g. run `npm start` when accessing `my.frontend.test`).
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

## Project status

Being rewritten from (almost) scratch.

## Contributors

* Big credit goes to [Emil Hernvall][emil] for his great [dnsguide][]. The
  entire DNS implementation is copied (with slight modifications) from his guide
  with his permission.

## Breaking Changes

### 0.3.0-beta1

* Proxy configuration has changed from `proxy:http://hostname:port/` to
  `proxy:hostname:post`. No auto conversion.

### 0.4.0-beta1

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

### 0.7

Complete rewrite. **Breaks everything**.

[latest]: https://github.com/babysnakes/duwop/releases/latest
[emil]: https://github.com/EmilHernvall
[dnsguide]: https://github.com/EmilHernvall/dnsguide
