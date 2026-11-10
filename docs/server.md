# Run Pullbay on a server

Pullbay's downloads run in a background program, the daemon (`pullbay-daemon`). On a desktop the
browser, the window or `pullbay-cli` starts it when it is needed, and it exits once it has had
nothing to do for a minute. On a home server or a NAS you start it yourself in **server mode**:

```
pullbay-daemon --server [--config DIR] [--data DIR] [--downloads DIR]
```

In server mode the daemon:

- **stays running** until it is stopped, with or without downloads;
- **takes its folders** from the command line or the environment (below);
- **logs to standard output**, one line per event, for `docker logs`, `journalctl` or whatever
  started it, instead of writing `pullbay-daemon.log`;
- **shows nothing on a screen**: it starts no window, shows no notification, plays no sound and
  does not register with the browsers. A finished or failed download is a line in the log;
- **stops cleanly** on `SIGTERM` or `SIGINT` (`docker stop`, `systemctl stop`, Ctrl+C), saving the
  download list first. Downloads that were running load as interrupted and resume with
  `pullbay-cli resume --all`, or by themselves when `resume_at_startup` is on.

It still listens on `127.0.0.1` only, with the same token handshake as on a desktop (see
[How the daemon checks its clients](architecture.md#host-daemon-authentication)). Server mode
opens nothing to the network.

## Folders

| | Option | Environment variable | Holds |
| --- | --- | --- | --- |
| settings folder | `--config DIR` | `PULLBAY_CONFIG_DIR` | `settings.json`, themes, the grabber's projects |
| data folder | `--data DIR` | `PULLBAY_DATA_DIR` | the download list, the daemon's token, the keys that seal saved logins, problem reports, the other programs' logs |
| downloads folder | `--downloads DIR` | `PULLBAY_DOWNLOADS_DIR` | downloads, when the settings name no folder (`download_dir`) |

An option wins over its variable; a folder named neither way is the usual one for the user the
daemon runs as (see [How Pullbay fits together](architecture.md#the-three-programs)). A relative
`DIR` on the command line is taken from the current folder. A folder named outright is used as
given, also with `PULLBAY_INSTANCE` set (which still picks the daemon's port).

`--config`, `--data` and `--downloads` are only for `--server`. The variables work for every
Pullbay program, which is what the other programs need: **`pullbay-cli` and `pullbay-mcp` find
the daemon's token in the data folder**, so give them the same `PULLBAY_DATA_DIR` (or run the
daemon with the usual data folder). The Docker image sets all three variables for every program in
the container.

## Docker

The [`Dockerfile`](../Dockerfile) at the top of the repository builds an image with
`pullbay-daemon`, `pullbay-cli` and `pullbay-mcp` on Debian. The release workflow publishes it for
each release as `ghcr.io/tajbinkhan/pullbay` (`latest`, the version, and `major.minor`), for
`linux/amd64`.

```
docker run -d --name pullbay --restart unless-stopped \
  -v pullbay-config:/config \
  -v /srv/downloads:/downloads \
  -e TZ=Europe/Berlin \
  ghcr.io/tajbinkhan/pullbay:latest
```

| In the container | What it is |
| --- | --- |
| `/config` (volume) | the settings folder; the data folder is `/config/data` |
| `/downloads` (volume) | the downloads folder |
| `TZ` | the time zone queue schedules use (default UTC) |
| user | `pullbay`, uid and gid 1000. Run with `--user UID:GID` to own the files as someone else; both folders must be writable by that user |

No port is published, and publishing one would not help: the daemon listens on the container's own
`127.0.0.1`.

[`installer/docker/compose.yaml`](../installer/docker/compose.yaml) is the same as a Compose file:

```
docker compose -f installer/docker/compose.yaml up -d
```

### Unraid

[`installer/docker/pullbay-unraid.xml`](../installer/docker/pullbay-unraid.xml) is a template.
Copy it to `/boot/config/plugins/dockerMan/templates-user/` on the flash drive, then add the
container from **Docker > Add Container** and pick `pullbay` under **Template**. It keeps the
settings in `/mnt/user/appdata/pullbay`, downloads into `/mnt/user/downloads` and runs as
`99:100` (nobody:users), as Unraid's shares expect.

### Building the image yourself

```
docker build -t pullbay .
docker build -t pullbay --build-arg FFMPEG=1 .   # with ffmpeg, for HLS and DASH streams
```

The published image has no ffmpeg, so a stream whose video and audio come apart is not joined (see
[Streams, batches and the site grabber](streams-batches-and-grabber.md)). There is no published
`arm64` image yet; on an arm64 machine, `docker build` there builds one. The build compiles the
whole workspace, the window's code too, so it takes a while and a few GB of memory.

## Adding and managing downloads

Run `pullbay-cli` ([The command-line tool](command-line.md#the-command-line-tool)) inside the
container:

```
docker exec pullbay pullbay-cli get https://example.com/file.iso --no-wait
docker exec pullbay pullbay-cli get https://example.com/file.iso -d /downloads/iso -c 8 --no-wait
docker exec pullbay pullbay-cli list
docker exec pullbay pullbay-cli pause 3
docker exec pullbay pullbay-cli resume --all
docker exec pullbay pullbay-cli queue start
docker logs -f pullbay
```

Without `--no-wait`, `get` shows the download's progress until it ends. A download added with
`pullbay-cli` never waits for a save-as dialog.

An AI assistant on the same machine can manage the downloads through `pullbay-mcp`
([AI assistants (MCP)](ai-assistants.md)): give it `docker` as the `command` and
`["exec", "-i", "pullbay", "pullbay-mcp"]` as the `args`.

Outside Docker, run `pullbay-cli` as the same user, with the same `PULLBAY_DATA_DIR` as the daemon.

## Settings

The settings are `settings.json` in the settings folder. A missing file or field means the
default, so a file can hold only what it changes:

```json
{
  "max_concurrent": 3,
  "connections": 8,
  "speed_limit_enabled": true,
  "speed_limit_kbps": 20480,
  "resume_at_startup": true
}
```

Stop the daemon before editing the file: it writes the file again when its settings change. The
window's Options page edits the same file, so a folder you share with a desktop copy of Pullbay
can be set up there.

On a server, a few settings work differently:

- Notifications and sounds (`notify`, `sound_on_done`, `sound_on_failed`) have no effect.
- A download from the browser extension with the save-as dialog on (`ask_where_to_save`) waits 20
  seconds for a window to answer, then starts with the proposed name and folder.
- A queue's "when done" shut down, sleep or hibernate needs a system that lets the daemon do that;
  a container does not.
- "Lift the speed limit while nobody uses the computer" sees no keyboard or mouse on a server.

## Running it without Docker

Any service manager that keeps a program running will do. A systemd unit, for a `pullbay` user:

```ini
[Unit]
Description=Pullbay download daemon
After=network-online.target
Wants=network-online.target

[Service]
User=pullbay
Environment=PULLBAY_CONFIG_DIR=/var/lib/pullbay/config
Environment=PULLBAY_DATA_DIR=/var/lib/pullbay/data
Environment=PULLBAY_DOWNLOADS_DIR=/srv/downloads
ExecStart=/usr/lib/pullbay/pullbay-daemon --server
Restart=on-failure

[Install]
WantedBy=multi-user.target
```

The daemon refuses to start, and exits with an error, if another daemon already runs for that user
and `PULLBAY_INSTANCE`, so the service manager sees it fail.

## What does not work yet

- **The browser extension on another computer cannot send downloads to the server.** The extension
  talks to `pullbay-host` on its own computer, which talks to a daemon there over loopback; there
  is no way across the network yet. On the computer the browser runs on, a daemon in server mode
  started as your own user, with the usual data folder, does take the extension's downloads, since
  the host finds its token as usual (the host must be registered with the browser already, by an
  installed Pullbay; server mode registers nothing).
- **A phone cannot reach it either.** Adding and watching downloads from a phone or another
  computer on the home network is planned as a paired, HTTPS page served by the daemon
  ([#254](https://github.com/tajbinkhan/pullbay/issues/254)). Until then, use `pullbay-cli` over
  `docker exec` or SSH.
- Settings other than the folders come from `settings.json` only, not from options or variables.
