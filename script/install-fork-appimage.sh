#!/usr/bin/env bash
# Download the latest fork AppImage release and install it as "Zed Fork".
# Re-run to update.
#
#   script/install-fork-appimage.sh

set -euo pipefail

REPO="${ZED_FORK_REPO:-hehenugas/zed-fork}"
BIN_DIR="$HOME/Applications"
APPS_DIR="$HOME/.local/share/applications"
ICON_DIR="$HOME/.local/share/icons/hicolor/512x512/apps"

log() { printf '\033[0;36m[zed-fork]\033[0m %s\n' "$1"; }
fail() { printf '\033[0;31m[zed-fork] %s\033[0m\n' "$1" >&2; exit 1; }
command -v gh >/dev/null || fail "gh (GitHub CLI) not found"

TMP="$(mktemp -d)"
trap 'rm -rf "$TMP"' EXIT
log "Downloading the latest release"
TAG="$(gh release list --repo "$REPO" --limit 1 --json tagName --jq '.[0].tagName')"
[ -n "$TAG" ] || fail "no release found in $REPO"
gh release download "$TAG" --repo "$REPO" --dir "$TMP" --pattern '*.AppImage' --pattern SHA256SUMS.txt
(cd "$TMP" && sha256sum -c SHA256SUMS.txt) || fail "checksum mismatch"
SRC="$(ls "$TMP"/zed-fork_*_x86_64.AppImage)"
OUT="$BIN_DIR/zed-fork.AppImage"

mkdir -p "$BIN_DIR" "$APPS_DIR" "$ICON_DIR"
install -Dm755 "$SRC" "$OUT"

"$OUT" --appimage-extract zed.png >/dev/null 2>&1 && mv squashfs-root/zed.png "$ICON_DIR/zed-fork.png" && rmdir squashfs-root 2>/dev/null || true

cat > "$APPS_DIR/zed-fork.desktop" <<DESK
[Desktop Entry]
Type=Application
Name=Zed Fork
Comment=Zed fork with a VS Code style UI
Exec=$OUT %U
Icon=zed-fork
Terminal=false
Categories=Development;TextEditor;
StartupWMClass=dev.zed.Zed-Fork
DESK
update-desktop-database "$APPS_DIR" 2>/dev/null || true
mkdir -p "$HOME/.local/bin" && ln -sf "$OUT" "$HOME/.local/bin/zed-fork"

log "Installed: $OUT (also as ~/.local/bin/zed-fork)"
