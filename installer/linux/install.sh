#!/bin/sh
# Installs Pullbay from this folder for the current user, without root:
#
#   ./install.sh            install into ~/.local
#   ./install.sh --remove   remove it again
#
# The programs go to ~/.local/lib/pullbay, `pullbay` to ~/.local/bin, and Pullbay
# shows up among the applications. Pullbay registers itself with the browsers
# it finds the first time it runs.
set -eu
HERE="$(cd "$(dirname "$0")" && pwd)"
DATA="${XDG_DATA_HOME:-$HOME/.local/share}"
LIB="$HOME/.local/lib/pullbay"

if [ "${1:-}" = "--remove" ]; then
  rm -rf "$LIB" "$HOME/.local/bin/pullbay" "$DATA/applications/pullbay.desktop" "$DATA/icons/hicolor/128x128/apps/pullbay.png"
  echo "Removed Pullbay. Your settings and download list stay in ~/.config/pullbay and $DATA/pullbay."
  exit 0
fi

# Pullbay was called UIDM up to 0.3.3. Its programs go; its settings and
# list move to Pullbay's folders when Pullbay first runs.
rm -rf "$HOME/.local/lib/uidm" "$HOME/.local/bin/uidm" "$DATA/applications/uidm.desktop" "$DATA/icons/hicolor/128x128/apps/uidm.png"
mkdir -p "$LIB" "$HOME/.local/bin" "$DATA/applications" "$DATA/icons/hicolor/128x128/apps"
for f in pullbay-gui pullbay-daemon pullbay-host README.md; do
  cp -f "$HERE/$f" "$LIB/"
done
rm -rf "$LIB/extension" "$LIB/firefox"
cp -R "$HERE/extension" "$HERE/firefox" "$LIB/"
ln -sf "$LIB/pullbay-gui" "$HOME/.local/bin/pullbay"
cp -f "$HERE/pullbay.png" "$DATA/icons/hicolor/128x128/apps/pullbay.png"
sed "s|^Exec=.*|Exec=\"$LIB/pullbay-gui\"|" "$HERE/pullbay.desktop" > "$DATA/applications/pullbay.desktop"
echo "Installed Pullbay in $LIB."
echo "Load the browser extension from $LIB/extension (Chrome, Edge, Brave) or $LIB/firefox (Firefox)."
