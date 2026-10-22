#!/bin/bash
# Builds "Strong Hold.app" (into ~/Applications, or the folder given as $1); --dmg also packs
# "Strong Hold.dmg" next to it. README: "Mac app".
# The app carries nothing of Firefly's: on first start it finds the user's copy of the game (CrossOver/Steam, a chosen
# folder) or downloads it from Steam with their account (Valve's DepotDownloader, bundled below), then exports what it
# needs once (extract/, ~2 min) into ~/Library/Application Support/Strong Hold.
set -euo pipefail
cd "$(dirname "$0")"
DMG=0; [ "${1:-}" = "--dmg" ] && { DMG=1; shift; }
OUT="${1:-$HOME/Applications}"
NAME="Strong Hold"
VERSION="${SHC_VERSION:-0.1.0}" # scripts/publish.sh sets it
APP="$OUT/$NAME.app"
# DepotDownloader 3.4.0 (SteamRE, GPL-2.0): self-contained macOS arm64 build, pinned by hash
DD_URL="https://github.com/SteamRE/DepotDownloader/releases/download/DepotDownloader_3.4.0/DepotDownloader-macos-arm64.zip"
DD_SHA="60e80c7c496f3f9a079cd3c62036b35d088c27bc0149baf38f009eb57a52f6a5"
DD_ZIP="$HOME/Library/Caches/shc-build/DepotDownloader-3.4.0-macos-arm64.zip"

command -v cargo-about >/dev/null || { echo "needs cargo-about: cargo install --locked cargo-about --features cli"; exit 1; }
SHC_VERSION="$VERSION" cargo build --release # update.rs compares it with the latest release
mkdir -p "$(dirname "$DD_ZIP")"
[ -f "$DD_ZIP" ] || curl -fsSL "$DD_URL" -o "$DD_ZIP"
echo "$DD_SHA  $DD_ZIP" | shasum -a 256 -c - >/dev/null || { echo "DepotDownloader download doesn't match its pinned hash"; rm -f "$DD_ZIP"; exit 1; }

rm -rf "$APP"
mkdir -p "$APP/Contents/MacOS" "$APP/Contents/Resources"
cp target/release/shc-viewer "$APP/Contents/MacOS/"
unzip -q -o "$DD_ZIP" DepotDownloader LICENSE -d "$APP/Contents/MacOS"
mv "$APP/Contents/MacOS/LICENSE" "$APP/Contents/Resources/LICENSE-DepotDownloader.txt"
cp steam_files.txt "$APP/Contents/Resources/"
# licence notices of every crate linked in (about.toml, about.hbs) + libvpx (static, extract/build.rs)
cargo about -L error generate --locked about.hbs -o "$APP/Contents/Resources/THIRD-PARTY-NOTICES.txt"
cat licenses/libvpx.txt >> "$APP/Contents/Resources/THIRD-PARTY-NOTICES.txt"
ICONSET="$(mktemp -d)/AppIcon.iconset"; mkdir -p "$ICONSET"
for px in 16 32 128 256 512; do
    sips -z $px $px icon/AppIcon.png --out "$ICONSET/icon_${px}x${px}.png" >/dev/null
    sips -z $((px * 2)) $((px * 2)) icon/AppIcon.png --out "$ICONSET/icon_${px}x${px}@2x.png" >/dev/null
done
iconutil -c icns "$ICONSET" -o "$APP/Contents/Resources/AppIcon.icns"
if otool -L target/release/shc-viewer | grep -q /opt/homebrew; then echo "shc-viewer links a Homebrew dylib (otool -L): the app wouldn't run on other Macs"; exit 1; fi

# Finder starts shc-viewer itself: no shell script in the bundle (paths::app_bundle_start does its old jobs: saves
# folder, log file, the pre-rename folder)

cat > "$APP/Contents/Info.plist" <<PLIST
<?xml version="1.0" encoding="UTF-8"?>
<!DOCTYPE plist PUBLIC "-//Apple//DTD PLIST 1.0//EN" "http://www.apple.com/DTDs/PropertyList-1.0.dtd">
<plist version="1.0">
<dict>
    <key>CFBundleName</key><string>Strong Hold</string>
    <key>CFBundleDisplayName</key><string>Strong Hold</string>
    <key>CFBundleIdentifier</key><string>local.shc.rust</string>
    <key>CFBundleExecutable</key><string>shc-viewer</string>
    <key>CFBundleIconFile</key><string>AppIcon</string>
    <key>CFBundlePackageType</key><string>APPL</string>
    <key>CFBundleShortVersionString</key><string>$VERSION</string>
    <key>CFBundleVersion</key><string>1</string>
    <key>LSApplicationCategoryType</key><string>public.app-category.strategy-games</string>
    <key>LSMinimumSystemVersion</key><string>12.0</string>
    <key>NSHighResolutionCapable</key><true/>
</dict>
</plist>
PLIST

codesign --force --deep -s - "$APP" # ad-hoc: other Macs need right-click > Open once (README); notarizing needs a Developer ID
echo "built $APP ($(du -sh "$APP" | cut -f1))"

if [ $DMG = 1 ]; then
    STAGE="$(mktemp -d)"
    cp -R "$APP" "$STAGE/"
    ln -s /Applications "$STAGE/Applications"
    hdiutil create -quiet -volname "$NAME" -srcfolder "$STAGE" -ov -format UDZO "$OUT/$NAME.dmg"
    rm -rf "$STAGE"
    echo "built $OUT/$NAME.dmg ($(du -sh "$OUT/$NAME.dmg" | cut -f1))"
fi
