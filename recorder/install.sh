#!/bin/sh
# Install the proxy into the CrossOver game folder. Safe to re-run after a Steam update:
# if CrusaderDE.dll is not our proxy (Steam replaced it), it becomes the new CrusaderDE_orig.dll.
# Undo: ./install.sh --undo   (or Steam -> Verify integrity of game files)
set -e
R="$(cd "$(dirname "$0")" && pwd)"
P="$HOME/Library/Application Support/CrossOver/Bottles/Steam/drive_c/Program Files (x86)/Steam/steamapps/common/Stronghold Crusader Definitive Edition/Stronghold Crusader Definitive Edition_Data/Plugins/x86_64"
cd "$P"
if [ "$1" = "--undo" ]; then mv -f CrusaderDE_orig.dll CrusaderDE.dll; echo restored original; exit; fi
if ! grep -qa "CrusaderDE_orig" CrusaderDE.dll; then mv -f CrusaderDE.dll CrusaderDE_orig.dll; echo "original -> CrusaderDE_orig.dll"; fi
cp "$R/target/x86_64-pc-windows-gnu/release/CrusaderDE.dll" CrusaderDE.dll
echo "proxy installed"
