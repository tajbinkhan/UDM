# The update host

For maintainers. UDM can look for updates on a Cloudflare R2 bucket before it
asks GitHub, so a network GitHub turns away still gets updates. The bucket
holds only the newest release. GitHub keeps every release and stays the
fallback.

## What the bucket holds

```
latest.json                         {"tag":"v0.12.0"}
releases/v0.12.0/SHA256SUMS
releases/v0.12.0/SHA256SUMS.minisig
releases/v0.12.0/UDM-Setup-0.12.0.exe
releases/v0.12.0/...                the release's other files
```

## What happens on a release

The Release workflow publishes the GitHub release first. Its `update-host` job
then runs `installer/publish-update-host.sh`, in this order:

1. It downloads the release from GitHub and checks the signature against the
   key UDM pins.
2. It uploads the files to `releases/<tag>/`.
3. It points `latest.json` at the new tag.
4. It deletes every other folder under `releases/`.

A pre-release stays on GitHub. Building an older tag again changes nothing on
the bucket. If the job fails, the GitHub release is already out and UDM updates
from there.

## What UDM does

UDM reads `latest.json`, then that tag's `SHA256SUMS` and signature. It believes
the host only when the signature is by UDM's release key and the list names
files of that version. When the host has nothing newer than the running copy,
UDM asks GitHub too, so a release the host missed still reaches people.

**Update now** downloads from the host and checks the file against the signed
list. If the file is missing or fails the check, UDM downloads it from GitHub.
That covers a copy that read the old `latest.json` just before the old release
was deleted.

A hacked bucket cannot make UDM install a changed file, or a version older than
the running copy. It could keep offering a real signed release that is newer
than the running copy but not the newest, until that copy has installed it.
From then on UDM asks GitHub as well and finds the newest release.

## Setting it up

1. In Cloudflare, create an R2 bucket.
2. Give the bucket a custom domain, such as `updates.example.com`. The
   `r2.dev` address is rate limited and meant for testing.
3. Create an R2 API token with **Object Read & Write** on that bucket.
4. In the GitHub repository, under **Settings > Secrets and variables >
   Actions**, add these secrets:

   | Secret | Value |
   | --- | --- |
   | `R2_ACCOUNT_ID` | The Cloudflare account ID |
   | `R2_ACCESS_KEY_ID` | The token's access key ID |
   | `R2_SECRET_ACCESS_KEY` | The token's secret access key |
   | `R2_BUCKET` | The bucket's name |

5. On the **Variables** tab of the same page, add `UDM_UPDATE_HOST` with the
   address, such as `https://updates.example.com`.

The address is built into UDM, so copies start using the host from the first
release built with the variable. Older copies keep updating from GitHub until
they install that release, so every release must stay on GitHub.

---

[← Back to the help pages](README.md)
