#!/bin/sh
set -eu
cd "$(dirname "$0")/.."
profile="${1:-release}"
case "$profile" in
  release) cargo build --locked --release ;;
  debug) cargo build --locked ;;
  *) echo 'Usage: scripts/bundle-macos.sh [debug|release]' >&2; exit 2 ;;
esac
bundle="target/bundle/Tiny RDM.app"
mkdir -p "$bundle/Contents/MacOS" "$bundle/Contents/Resources"
cp "target/$profile/rust-rdm" "$bundle/Contents/MacOS/rust-rdm"
cp assets/images/icon.png "$bundle/Contents/Resources/icon.png"
cp assets/images/AppIcon.icns "$bundle/Contents/Resources/AppIcon.icns"
frameworks="$bundle/Contents/Frameworks"
rm -rf "$frameworks"
mkdir -p "$frameworks"
local_libraries=$(otool -L "$bundle/Contents/MacOS/rust-rdm" | awk '$1 ~ /^\/opt\/homebrew\// || $1 ~ /^\/usr\/local\// {print $1}')
for library in $local_libraries; do
  library_name=$(basename "$library")
  cp "$library" "$frameworks/$library_name"
  chmod u+w "$frameworks/$library_name"
  install_name_tool -id "@rpath/$library_name" "$frameworks/$library_name"
  install_name_tool -change "$library" "@rpath/$library_name" "$bundle/Contents/MacOS/rust-rdm"
done
if [ -n "$local_libraries" ]; then
  install_name_tool -add_rpath '@executable_path/../Frameworks' "$bundle/Contents/MacOS/rust-rdm"
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
  vtool -show-build "$bundle/Contents/MacOS/rust-rdm"
  for bundled_library in "$frameworks"/*.dylib; do
    [ ! -f "$bundled_library" ] || vtool -show-build "$bundled_library"
  done
} | awk '
  /minos/ { split($2, part, "."); number = part[1] * 10000 + part[2] * 100 + part[3]; if (number > maximum) { maximum = number; version = $2 } }
  END { print version }')
cat > "$bundle/Contents/Info.plist" <<PLIST
<?xml version="1.0" encoding="UTF-8"?>
<!DOCTYPE plist PUBLIC "-//Apple//DTD PLIST 1.0//EN" "http://www.apple.com/DTDs/PropertyList-1.0.dtd">
<plist version="1.0"><dict>
<key>CFBundleExecutable</key><string>rust-rdm</string>
<key>CFBundleIdentifier</key><string>com.tinycraft.rust-rdm</string>
<key>CFBundleName</key><string>Tiny RDM</string>
<key>CFBundleDisplayName</key><string>Tiny RDM</string>
<key>CFBundlePackageType</key><string>APPL</string>
<key>CFBundleShortVersionString</key><string>0.1.0</string>
<key>CFBundleVersion</key><string>1</string>
<key>CFBundleIconFile</key><string>AppIcon.icns</string>
<key>NSHighResolutionCapable</key><true/>
<key>LSMinimumSystemVersion</key><string>$minimum_system</string>
</dict></plist>
PLIST
codesign --force --deep --sign - "$bundle"
echo "$PWD/$bundle"
