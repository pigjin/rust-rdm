#!/bin/sh
set -eu
cd "$(dirname "$0")/.."
profile="${1:-release}"
xcrun --find actool >/dev/null
if ! xcodebuild -version | awk '/^Xcode / { found = 1; if ($2 + 0 >= 26) ok = 1 } END { exit !(found && ok) }'; then
  echo 'macOS app icons require full Xcode 26 or later (select it with DEVELOPER_DIR or xcode-select).' >&2
  exit 1
fi
case "$profile" in
  release) cargo build --locked --release ;;
  debug) cargo build --locked ;;
  *) echo 'Usage: scripts/bundle-macos.sh [debug|release]' >&2; exit 2 ;;
esac
version=$(python3 scripts/release.py version --core)
target_dir="${CARGO_TARGET_DIR:-target}"
binary_dir="$target_dir/${CARGO_BUILD_TARGET:+$CARGO_BUILD_TARGET/}$profile"
bundle="$target_dir/bundle/RedisDock.app"
mkdir -p "$bundle/Contents/MacOS" "$bundle/Contents/Resources"
cp "$binary_dir/redisdock" "$bundle/Contents/MacOS/redisdock"
cp assets/images/icon.png "$bundle/Contents/Resources/icon.png"
frameworks="$bundle/Contents/Frameworks"
rm -rf "$frameworks"
mkdir -p "$frameworks"
local_libraries=$(otool -L "$bundle/Contents/MacOS/redisdock" | awk '$1 ~ /^\/opt\/homebrew\// || $1 ~ /^\/usr\/local\// {print $1}')
for library in $local_libraries; do
  library_name=$(basename "$library")
  cp "$library" "$frameworks/$library_name"
  chmod u+w "$frameworks/$library_name"
  install_name_tool -id "@rpath/$library_name" "$frameworks/$library_name"
  install_name_tool -change "$library" "@rpath/$library_name" "$bundle/Contents/MacOS/redisdock"
done
if [ -n "$local_libraries" ]; then
  install_name_tool -add_rpath '@executable_path/../Frameworks' "$bundle/Contents/MacOS/redisdock"
  for bundled_library in "$frameworks"/*.dylib; do
    for library in $local_libraries; do
      install_name_tool -change "$library" "@rpath/$(basename "$library")" "$bundled_library"
    done
    codesign --force --sign - "$bundled_library"
  done
fi
for third_party_license in assets/licenses/*.txt; do
  [ ! -f "$third_party_license" ] || cp "$third_party_license" "$bundle/Contents/Resources/"
done
cp LICENSE "$bundle/Contents/Resources/LICENSE"
minimum_system=$({
  vtool -show-build "$bundle/Contents/MacOS/redisdock"
  for bundled_library in "$frameworks"/*.dylib; do
    [ ! -f "$bundled_library" ] || vtool -show-build "$bundled_library"
  done
} | awk '
  /minos/ { split($2, part, "."); number = part[1] * 10000 + part[2] * 100 + part[3]; if (number > maximum) { maximum = number; version = $2 } }
  END { print version }')
icon_info="$target_dir/macos-icon-info.plist"
if [ -n "${REDISDOCK_MACOS_ICON_DIR:-}" ]; then
  # Xcode 26.3 on the Intel runner fails to resolve this valid Icon Composer layer.
  for icon in Assets.car AppIcon.icns; do
    cp "$REDISDOCK_MACOS_ICON_DIR/$icon" "$bundle/Contents/Resources/$icon"
  done
  cp "$REDISDOCK_MACOS_ICON_DIR/icon-info.plist" "$icon_info"
else
  xcrun actool assets/images/AppIcon.icon \
    --compile "$bundle/Contents/Resources" --app-icon AppIcon \
    --platform macosx --minimum-deployment-target "$minimum_system" \
    --output-partial-info-plist "$icon_info" --output-format human-readable-text --warnings --notices
fi
cat > "$bundle/Contents/Info.plist" <<PLIST
<?xml version="1.0" encoding="UTF-8"?>
<!DOCTYPE plist PUBLIC "-//Apple//DTD PLIST 1.0//EN" "http://www.apple.com/DTDs/PropertyList-1.0.dtd">
<plist version="1.0"><dict>
<key>CFBundleExecutable</key><string>redisdock</string>
<key>CFBundleIdentifier</key><string>com.pigjin.redisdock</string>
<key>CFBundleName</key><string>RedisDock</string>
<key>CFBundleDisplayName</key><string>RedisDock</string>
<key>CFBundlePackageType</key><string>APPL</string>
<key>CFBundleShortVersionString</key><string>$version</string>
<key>CFBundleVersion</key><string>$version</string>
<key>NSHighResolutionCapable</key><true/>
<key>LSMinimumSystemVersion</key><string>$minimum_system</string>
</dict></plist>
PLIST
python3 - "$bundle/Contents/Info.plist" "$icon_info" <<'PY'
import plistlib
import sys
from pathlib import Path

metadata, icons = map(Path, sys.argv[1:])
info = plistlib.loads(metadata.read_bytes())
generated = plistlib.loads(icons.read_bytes())
assert generated.get('CFBundleIconName') == 'AppIcon', 'Missing native app icon metadata'
assert generated.get('CFBundleIconFile') in ('AppIcon', 'AppIcon.icns'), 'Missing fallback icon metadata'
info.update(generated)
metadata.write_bytes(plistlib.dumps(info))
PY
codesign --force --deep --sign - "$bundle"
touch "$bundle"
cd "$bundle"
pwd
