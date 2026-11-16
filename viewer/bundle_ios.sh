#!/bin/bash
# Builds the iOS/iPadOS app shell "Strong Hold.app" (into the folder given as $1, default target/ios): the binary,
# Info.plist, icon. Like the Mac app it carries nothing of Firefly's; the IPA builder (ipa.rs, the Mac app's
# --build-ipa) adds the player's exports to it and packs the .ipa. --sim builds for the iOS Simulator instead.
# Needs the rustup toolchain's iOS targets: rustup target add aarch64-apple-ios aarch64-apple-ios-sim
set -euo pipefail
cd "$(dirname "$0")"
TARGET=aarch64-apple-ios; PLATFORM=iPhoneOS
[ "${1:-}" = "--sim" ] && { TARGET=aarch64-apple-ios-sim; PLATFORM=iPhoneSimulator; shift; }
OUT="${1:-target/ios}"
VERSION="${SHC_VERSION:-0.1.0}"
APP="$OUT/Strong Hold.app"
# Homebrew's rustc has no iOS std: build with rustup's
RUSTC="$(rustup which rustc)" SHC_VERSION="$VERSION" "$(rustup which cargo)" build --release --target $TARGET

rm -rf "$APP"
mkdir -p "$APP"
cp target/$TARGET/release/shc-viewer "$APP/StrongHold" # not shc-viewer: that is the export folder beside it (paths::cache)
cp steam_files.txt "$APP/"
# winit 0.30 starts without UIScene, which apps linked against the iOS 27 SDK must adopt or they won't launch: claim
# the iOS 26 SDK (ponytail: drop once winit does scenes). Ad-hoc signed: the player's sideloader signs it for real.
P=$([ $PLATFORM = iPhoneOS ] && echo 2 || echo 7)
xcrun vtool -set-build-version $P 16.0 26.0 -replace -output "$APP/StrongHold.v" "$APP/StrongHold" && mv "$APP/StrongHold.v" "$APP/StrongHold"
sips -z 180 180 icon/AppIcon.png --out "$APP/AppIcon60x60@3x.png" >/dev/null
sips -z 167 167 icon/AppIcon.png --out "$APP/AppIcon83.5x83.5@2x~ipad.png" >/dev/null
sips -z 152 152 icon/AppIcon.png --out "$APP/AppIcon76x76@2x~ipad.png" >/dev/null
SDK_VER="$(xcrun --sdk "$(echo $PLATFORM | tr A-Z a-z)" --show-sdk-version)"
cat > "$APP/Info.plist" <<PLIST
<?xml version="1.0" encoding="UTF-8"?>
<!DOCTYPE plist PUBLIC "-//Apple//DTD PLIST 1.0//EN" "http://www.apple.com/DTDs/PropertyList-1.0.dtd">
<plist version="1.0">
<dict>
    <key>CFBundleName</key><string>Strong Hold</string>
    <key>CFBundleDisplayName</key><string>Strong Hold</string>
    <key>CFBundleIdentifier</key><string>local.shc.rust</string>
    <key>CFBundleExecutable</key><string>StrongHold</string>
    <key>CFBundlePackageType</key><string>APPL</string>
    <key>CFBundleShortVersionString</key><string>$VERSION</string>
    <key>CFBundleVersion</key><string>1</string>
    <key>CFBundleSupportedPlatforms</key><array><string>$PLATFORM</string></array>
    <key>CFBundleIcons</key><dict><key>CFBundlePrimaryIcon</key><dict><key>CFBundleIconFiles</key><array><string>AppIcon60x60</string></array></dict></dict>
    <key>CFBundleIcons~ipad</key><dict><key>CFBundlePrimaryIcon</key><dict><key>CFBundleIconFiles</key><array><string>AppIcon76x76</string><string>AppIcon83.5x83.5</string></array></dict></dict>
    <key>DTPlatformName</key><string>$(echo $PLATFORM | tr A-Z a-z)</string>
    <key>DTSDKName</key><string>$(echo $PLATFORM | tr A-Z a-z)$SDK_VER</string>
    <key>MinimumOSVersion</key><string>16.0</string>
    <key>LSRequiresIPhoneOS</key><true/>
    <key>UIDeviceFamily</key><array><integer>1</integer><integer>2</integer></array>
    <key>UIRequiredDeviceCapabilities</key><array><string>arm64</string><string>metal</string></array>
    <!-- full screen at the device's own size (no launch screen = old letterboxed mode) -->
    <key>UILaunchScreen</key><dict/>
    <key>UIRequiresFullScreen</key><true/>
    <key>UIStatusBarHidden</key><true/>
    <key>UIViewControllerBasedStatusBarAppearance</key><false/>
    <key>UISupportedInterfaceOrientations</key><array><string>UIInterfaceOrientationLandscapeLeft</string><string>UIInterfaceOrientationLandscapeRight</string></array>
    <key>UISupportedInterfaceOrientations~ipad</key><array><string>UIInterfaceOrientationLandscapeLeft</string><string>UIInterfaceOrientationLandscapeRight</string></array>
    <!-- LAN games (lan.rs) -->
    <key>NSLocalNetworkUsageDescription</key><string>Finds and joins LAN games on your network.</string>
</dict>
</plist>
PLIST
codesign --force -s - "$APP"
echo "$APP ($TARGET)"
