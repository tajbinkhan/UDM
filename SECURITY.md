# Security

## Reporting a vulnerability

Please don't report a security hole in a public issue, pull request or discussion.

Report it privately through GitHub instead:

1. Open the repository's [Security tab](https://github.com/tajbinkhan/pullbay/security).
2. Click **Report a vulnerability**
   ([direct link](https://github.com/tajbinkhan/pullbay/security/advisories/new)).
3. Describe the problem and send it.

Only you and the maintainer see the report. The fix is worked out there, and the advisory is
published when a release that fixes it is out.

Helpful things to include:

- The UDM version (**Help > About UDM**), your system, and how you installed it.
- What an attacker can do, and what they need first (a web page you visit, a program on the same
  computer, a file you open).
- Steps to reproduce it, or a proof of concept.

## Supported versions

Fixes go into the latest release only. UDM checks for updates itself, so please update before
you report.

## What counts

UDM handles downloads for the browser, so these matter most:

- The daemon, which listens on `127.0.0.1` and checks every client with a token and an HMAC
  handshake ([how](docs/architecture.md)).
- `udm-host`, the program the browser starts, and the messages it relays.
- The browser extension, and what a web page can make it do.
- Saved logins, cookies and requests, which UDM keeps sealed for your user.
- Where a download is saved, and file names a server or a page can choose.
- The update check and the installers.

Bugs that need someone already able to run programs as your user, or to change UDM's own
files, are ordinary bugs. Open an [issue](https://github.com/tajbinkhan/pullbay/issues) for those.
