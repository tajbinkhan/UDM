# Run UDM on a server

UDM's downloads run in a background program, the daemon (`udm-daemon`). On a desktop the
browser, the window or `udm-cli` starts it when it is needed, and it exits once it has had
nothing to do for a minute. On a home server or a NAS you start it yourself in **server mode**:

```
udm-daemon --server [--config DIR] [--data DIR] [--downloads DIR]
```

In server mode the daemon:

- **stays running** until it is stopped, with or without downloads;
- **takes its folders** from the command line or the environment (below);
- **logs to standard output**, one line per event, for `docker logs`, `journalctl` or whatever
  started it, instead of writing `udm-daemon.log`;
- **shows nothing on a screen**: it starts no window, shows no notification, plays no sound and
  does not register with the browsers. A finished or failed download is a line in the log;
- **stops cleanly** on `SIGTERM` or `SIGINT` (`docker stop`, `systemctl stop`, Ctrl+C), saving the
  download list first. Downloads that were running load as interrupted and resume with
  `udm-cli resume --all`, or by themselves when `resume_at_startup` is on.

It still listens on `127.0.0.1` only, with the same token handshake as on a desktop (see
[How the daemon checks its clients](architecture.md#host-daemon-authentication)). Server mode
opens nothing to the network.

## Folders

| | Option | Environment variable | Holds |
| --- | --- | --- | --- |
| settings folder | `--config DIR` | `UDM_CONFIG_DIR` | `settings.json`, themes, the grabber's projects |
| data folder | `--data DIR` | `UDM_DATA_DIR` | the download list, the daemon's token, the keys that seal saved logins, problem reports, the other programs' logs |
| downloads folder | `--downloads DIR` | `UDM_DOWNLOADS_DIR` | downloads, when the settings name no folder (`download_dir`) |

An option wins over its variable; a folder named neither way is the usual one for the user the
daemon runs as (see [How UDM fits together](architecture.md#the-three-programs)). A relative
`DIR` on the command line is taken from the current folder. A folder named outright is used as
given, also with `UDM_INSTANCE` set (which still picks the daemon's port).

`--config`, `--data` and `--downloads` are only for `--server`. The variables work for every
UDM program, which is what the other programs need: **`udm-cli` and `udm-mcp` find
the daemon's token in the data folder**, so give them the same `UDM_DATA_DIR` (or run the
daemon with the usual data folder). The Docker image sets all three variables for every program in
the container.

## Docker

The [`Dockerfile`](../Dockerfile) at the top of the repository builds an image with
`udm-daemon`, `udm-cli` and `udm-mcp` on Debian. The release workflow publishes it for
each release as `ghcr.io/tajbinkhan/udm` (`latest`, the version, and `major.minor`), for
`linux/amd64`.

```
docker run -d --name udm --restart unless-stopped \
  -v udm-config:/config \
  -v /srv/downloads:/downloads \
  -e TZ=Europe/Berlin \
  ghcr.io/tajbinkhan/udm:latest
```

| In the container | What it is |
| --- | --- |
| `/config` (volume) | the settings folder; the data folder is `/config/data` |
| `/downloads` (volume) | the downloads folder |
| `TZ` | the time zone queue schedules use (default UTC) |
| user | `udm`, uid and gid 1000. Run with `--user UID:GID` to own the files as someone else; both folders must be writable by that user |

No port is published, and publishing one would not help: the daemon listens on the container's own
`127.0.0.1`.

[`installer/docker/compose.yaml`](../installer/docker/compose.yaml) is the same as a Compose file:

```
docker compose -f installer/docker/compose.yaml up -d
```

### Unraid

[`installer/docker/udm-unraid.xml`](../installer/docker/udm-unraid.xml) is a template.
Copy it to `/boot/config/plugins/dockerMan/templates-user/` on the flash drive, then add the
container from **Docker > Add Container** and pick `udm` under **Template**. It keeps the
settings in `/mnt/user/appdata/udm`, downloads into `/mnt/user/downloads` and runs as
`99:100` (nobody:users), as Unraid's shares expect.

### Building the image yourself

```
docker build -t udm .
docker build -t udm --build-arg FFMPEG=1 .   # with ffmpeg, for HLS and DASH streams
```

The published image has no ffmpeg, so a stream whose video and audio come apart is not joined (see
[Streams, batches and the site grabber](streams-batches-and-grabber.md)). There is no published
`arm64` image yet; on an arm64 machine, `docker build` there builds one. The build compiles the
whole workspace, the window's code too, so it takes a while and a few GB of memory.

## Adding and managing downloads

Run `udm-cli` ([The command-line tool](command-line.md#the-command-line-tool)) inside the
container:

```
docker exec udm udm-cli get https://example.com/file.iso --no-wait
docker exec udm udm-cli get https://example.com/file.iso -d /downloads/iso -c 8 --no-wait
docker exec udm udm-cli list
docker exec udm udm-cli pause 3
docker exec udm udm-cli resume --all
docker exec udm udm-cli queue start
docker logs -f udm
```

Without `--no-wait`, `get` shows the download's progress until it ends. A download added with
`udm-cli` never waits for a save-as dialog.

An AI assistant on the same machine can manage the downloads through `udm-mcp`
([AI assistants (MCP)](ai-assistants.md)): give it `docker` as the `command` and
`["exec", "-i", "udm", "udm-mcp"]` as the `args`.

Outside Docker, run `udm-cli` as the same user, with the same `UDM_DATA_DIR` as the daemon.

## From a phone

A server's daemon serves the [remote-control page](remote-control.md) as a desktop's does, so a
phone on the home network can list, add, pause, resume and remove downloads. It is off until the
settings turn it on:

1. Stop the container, add `"remote_enabled": true` to `settings.json` in the settings folder
   (`/config/settings.json` in the image; `remote_port` changes the port, 18443 by default), and
   start it again.
2. Publish the port: uncomment the `ports` lines in `installer/docker/compose.yaml` (or add
   `-p 18443:18443` to `docker run`). Publish it on the home network only, for example
   `"192.168.1.20:18443:18443"` with the server's own address, and never forward it on the router.
   With `network_mode: host` instead, nothing needs publishing.
3. Pair the phone from a terminal, giving the address the phone reaches the server at (inside a
   container the daemon sees only the container's own):

   ```
   docker exec -it udm udm-cli pair --host 192.168.1.20
   ```

   Scan the QR code it prints, check the certificate's fingerprint in the phone's warning against
   the one printed, and go on. `udm-cli devices` lists the paired phones and `udm-cli
   unpair ID` removes one at once.

The page's certificate and the paired devices are kept in the data folder's `remote` folder
(`/config/data/remote` in the image), sealed and readable by the daemon's user only. See
[Remote control](remote-control.md) for the threat model.

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
window's Options page edits the same file, so a folder you share with a desktop copy of UDM
can be set up there.

On a server, a few settings work differently:

- Notifications and sounds (`notify`, `sound_on_done`, `sound_on_failed`) have no effect.
- A download from the browser extension with the save-as dialog on (`ask_where_to_save`) waits 20
  seconds for a window to answer, then starts with the proposed name and folder.
- A queue's "when done" shut down, sleep or hibernate needs a system that lets the daemon do that;
  a container does not.
- "Lift the speed limit while nobody uses the computer" sees no keyboard or mouse on a server.

## Running it without Docker

Any service manager that keeps a program running will do. A systemd unit, for a `udm` user:

```ini
[Unit]
Description=UDM download daemon
After=network-online.target
Wants=network-online.target

[Service]
User=udm
Environment=UDM_CONFIG_DIR=/var/lib/udm/config
Environment=UDM_DATA_DIR=/var/lib/udm/data
Environment=UDM_DOWNLOADS_DIR=/srv/downloads
ExecStart=/usr/lib/udm/udm-daemon --server
Restart=on-failure

[Install]
WantedBy=multi-user.target
```

The daemon refuses to start, and exits with an error, if another daemon already runs for that user
and `UDM_INSTANCE`, so the service manager sees it fail.

## What does not work yet

- **The browser extension on another computer cannot send downloads to the server.** The extension
  talks to `udm-host` on its own computer, which talks to a daemon there over loopback; there
  is no way across the network yet. On the computer the browser runs on, a daemon in server mode
  started as your own user, with the usual data folder, does take the extension's downloads, since
  the host finds its token as usual (the host must be registered with the browser already, by an
  installed UDM; server mode registers nothing).
- Settings other than the folders come from `settings.json` only, not from options or variables.
