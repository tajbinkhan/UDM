#!/usr/bin/env bash
# Puts a release on the update host, a Cloudflare R2 bucket that holds only
# the newest release (docs/update-host.md). GitHub keeps every release.
#
#   bash installer/publish-update-host.sh <folder of the release's files> <tag>
#
# R2_BUCKET names the bucket. The AWS command line reaches R2 through the
# AWS_ variables the release workflow sets.
#
# The order matters. The new files go up first, latest.json then points at
# them, and only then do the older releases go. A copy of UDM that read the
# old latest.json a moment ago finds its files gone and downloads from GitHub.

set -euo pipefail

dist="${1:?usage: publish-update-host.sh <folder> <tag>}"
tag="${2:?usage: publish-update-host.sh <folder> <tag>}"
bucket="s3://${R2_BUCKET:?R2_BUCKET is not set}"

if [[ ! "$tag" =~ ^v[0-9]+\.[0-9]+\.[0-9]+$ ]]; then
  echo "publish-update-host: $tag is not a release's tag" >&2
  exit 2
fi

# Building an older tag again must not replace a newer release.
current="$(aws s3 cp "$bucket/latest.json" - 2>/dev/null | sed -n 's/.*"tag" *: *"\([^"]*\)".*/\1/p' || true)"
if [ -n "$current" ] && [ "$(printf '%s\n%s\n' "$current" "$tag" | sort -V | tail -n 1)" != "$tag" ]; then
  echo "The update host already has $current, which is newer than $tag."
  exit 0
fi

aws s3 cp "$dist" "$bucket/releases/$tag/" --recursive --only-show-errors \
  --cache-control "public, max-age=86400"

latest="$(mktemp)"
trap 'rm -f "$latest"' EXIT
printf '{"tag":"%s"}\n' "$tag" > "$latest"
aws s3 cp "$latest" "$bucket/latest.json" --only-show-errors \
  --content-type application/json --cache-control "public, max-age=300"

aws s3 ls "$bucket/releases/" | awk '$1 == "PRE" { print $2 }' | while IFS= read -r folder; do
  if [ -n "$folder" ] && [ "$folder" != "$tag/" ]; then
    echo "Deleting releases/$folder"
    aws s3 rm "$bucket/releases/$folder" --recursive --only-show-errors
  fi
done

echo "The update host now holds $tag."
