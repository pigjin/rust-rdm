#!/bin/sh
set -eu
cd "$(dirname "$0")/.."
arch="${1:-$(uname -m)}"
case "$arch" in arm64|x86_64) ;; *) echo "Unsupported architecture: $arch" >&2; exit 2 ;; esac
version=$(python3 scripts/release.py version)
scripts/bundle-macos.sh release
python3 scripts/check-macos-bundle.py release
target_dir="${CARGO_TARGET_DIR:-target}"
bundle="$target_dir/bundle/RedisDock.app"
python3 scripts/release.py binary "$bundle/Contents/MacOS/redisdock" "$arch"
mkdir -p "$target_dir/dist"
destination="$PWD/$target_dir/dist/RedisDock-$version-macos-$arch.dmg"
case "$target_dir" in /*) destination="$target_dir/dist/RedisDock-$version-macos-$arch.dmg" ;; esac
work=$(mktemp -d "${TMPDIR:-/tmp}/redisdock-dmg.XXXXXX")
mounted=false
cleanup() {
  if [ "$mounted" = true ]; then hdiutil detach "$work/mount"; fi
  rm -rf "$work"
}
trap cleanup EXIT
mkdir "$work/staging" "$work/mount"
ditto "$bundle" "$work/staging/RedisDock.app"
ln -s /Applications "$work/staging/Applications"
hdiutil create -volname "RedisDock $version" -srcfolder "$work/staging" -format UDZO -ov "$destination"
hdiutil verify "$destination"
hdiutil attach -readonly -nobrowse -mountpoint "$work/mount" "$destination"
mounted=true
test "$(readlink "$work/mount/Applications")" = /Applications
cmp "$bundle/Contents/Info.plist" "$work/mount/RedisDock.app/Contents/Info.plist"
cmp "$bundle/Contents/MacOS/redisdock" "$work/mount/RedisDock.app/Contents/MacOS/redisdock"
codesign --verify --deep --strict "$work/mount/RedisDock.app"
test -s "$work/mount/RedisDock.app/Contents/Resources/LICENSE"
for license in assets/licenses/*.txt; do
  cmp "$license" "$work/mount/RedisDock.app/Contents/Resources/$(basename "$license")"
done
echo "DMG PASS: $destination"
