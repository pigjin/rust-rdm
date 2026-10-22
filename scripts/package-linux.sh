#!/usr/bin/env bash
set -euo pipefail
cd "$(dirname "$0")/.."
arch="${1:-$(uname -m)}"
case "$arch" in
  x86_64)
    tool_arch=x86_64; deb_arch=amd64
    linuxdeploy_sha=c20cd71e3a4e3b80c3483cef793cda3f4e990aca14014d23c544ca3ce1270b4d
    appimagetool_sha=ed4ce84f0d9caff66f50bcca6ff6f35aae54ce8135408b3fa33abfc3cb384eb0
    runtime_sha=2fca8b443c92510f1483a883f60061ad09b46b978b2631c807cd873a47ec260d
    ;;
  arm64|aarch64)
    arch=arm64; tool_arch=aarch64; deb_arch=arm64
    linuxdeploy_sha=620095110d693282b8ebeb244a95b5e911cf8f65f76c88b4b47d16ae6346fcff
    appimagetool_sha=f0837e7448a0c1e4e650a93bb3e85802546e60654ef287576f46c71c126a9158
    runtime_sha=00cbdfcf917cc6c0ff6d3347d59e0ca1f7f45a6df1a428a0d6d8a78664d87444
    ;;
  *) echo "Unsupported architecture: $arch" >&2; exit 2 ;;
esac
version=$(python3 scripts/release.py version)
deb_version=$(python3 scripts/release.py version --deb)
target_dir=$(realpath -m "${CARGO_TARGET_DIR:-target}")
binary="$target_dir/${CARGO_BUILD_TARGET:+$CARGO_BUILD_TARGET/}release/redisdock"
python3 scripts/release.py binary "$binary" "$arch"
desktop-file-validate scripts/redisdock.desktop
mkdir -p "$target_dir/dist"
work=$(mktemp -d)
trap 'rm -rf "$work"' EXIT

download() {
  local url="$1" checksum="$2" destination="$3"
  curl --fail --location --retry 3 --output "$destination" "$url"
  printf '%s  %s\n' "$checksum" "$destination" | sha256sum --check --status
  chmod +x "$destination"
}
extract_tool() {
  local project="$1" tag="$2" name="$3" checksum="$4"
  download "https://github.com/$project/releases/download/$tag/$name-$tool_arch.AppImage" "$checksum" "$work/$name.AppImage"
  mkdir "$work/$name"
  (cd "$work/$name"; "$work/$name.AppImage" --appimage-extract >/dev/null)
}
# Use immutable release tags and checked digests, including the AppImage runtime.
extract_tool linuxdeploy/linuxdeploy 1-alpha-20251107-1 linuxdeploy "$linuxdeploy_sha"
extract_tool AppImage/appimagetool 1.9.1 appimagetool "$appimagetool_sha"
download "https://github.com/AppImage/type2-runtime/releases/download/20251108/runtime-$tool_arch" "$runtime_sha" "$work/runtime"

debroot="$work/deb"
convert assets/images/icon.png -resize 256x256 "$work/redisdock.png"
mkdir -p "$debroot/DEBIAN" "$debroot/usr/bin" "$debroot/usr/share/applications" \
  "$debroot/usr/share/icons/hicolor/256x256/apps" "$debroot/usr/share/doc/redisdock/licenses" "$work/debian"
install -m755 "$binary" "$debroot/usr/bin/redisdock"
install -m644 scripts/redisdock.desktop "$debroot/usr/share/applications/redisdock.desktop"
install -m644 "$work/redisdock.png" "$debroot/usr/share/icons/hicolor/256x256/apps/redisdock.png"
install -m644 LICENSE "$debroot/usr/share/doc/redisdock/copyright"
install -m644 assets/licenses/*.txt "$debroot/usr/share/doc/redisdock/licenses/"
cat > "$work/debian/control" <<CONTROL
Source: redisdock
Maintainer: RedisDock contributors

Package: redisdock
Architecture: any
Description: Native Redis desktop client
CONTROL
dependencies=$(cd "$work"; dpkg-shlibdeps -O -e "$binary")
dependencies="${dependencies#shlibs:Depends=}"
test -n "$dependencies"
cat > "$debroot/DEBIAN/control" <<CONTROL
Package: redisdock
Version: $deb_version
Architecture: $deb_arch
Maintainer: RedisDock contributors
Section: database
Priority: optional
Homepage: https://github.com/pigjin/rust-rdm
Depends: $dependencies, libvulkan1, libwayland-client0, libxkbcommon0, libxkbcommon-x11-0
Description: RedisDock, a native Redis desktop client built with Rust and GPUI
CONTROL
deb="$target_dir/dist/RedisDock-$version-linux-$arch.deb"
dpkg-deb --root-owner-group --build "$debroot" "$deb"
test "$(dpkg-deb -f "$deb" Version)" = "$deb_version"
test "$(dpkg-deb -f "$deb" Architecture)" = "$deb_arch"
dpkg-deb --extract "$deb" "$work/deb-check"
diff -qr "$debroot/usr" "$work/deb-check/usr"
test -x "$work/deb-check/usr/bin/redisdock"

appdir="$work/RedisDock.AppDir"
"$work/linuxdeploy/squashfs-root/AppRun" --appdir "$appdir" --executable "$binary" \
  --desktop-file scripts/redisdock.desktop --icon-file "$work/redisdock.png" \
  --library "$(pkg-config --variable=libdir fontconfig)/libfontconfig.so.1" \
  --library "$(pkg-config --variable=libdir wayland-client)/libwayland-client.so.0" \
  --library "$(pkg-config --variable=libdir xkbcommon)/libxkbcommon.so.0" \
  --library "$(pkg-config --variable=libdir xkbcommon-x11)/libxkbcommon-x11.so.0"
mkdir -p "$appdir/usr/share/doc/redisdock"
cp -R "$debroot/usr/share/doc/redisdock/." "$appdir/usr/share/doc/redisdock/"
appimage="$target_dir/dist/RedisDock-$version-linux-$arch.AppImage"
ARCH="$tool_arch" VERSION="$version" "$work/appimagetool/squashfs-root/AppRun" \
  --no-appstream --runtime-file "$work/runtime" "$appdir" "$appimage"
mkdir "$work/appimage-check"
(cd "$work/appimage-check"; "$appimage" --appimage-extract >/dev/null)
extracted="$work/appimage-check/squashfs-root"
cmp "$appdir/usr/bin/redisdock" "$extracted/usr/bin/redisdock"
diff -qr "$debroot/usr/share/doc/redisdock" "$extracted/usr/share/doc/redisdock"
desktop-file-validate "$extracted/redisdock.desktop"
python3 scripts/release.py binary "$extracted/usr/bin/redisdock" "$arch"
LD_LIBRARY_PATH="$extracted/usr/lib${LD_LIBRARY_PATH:+:$LD_LIBRARY_PATH}" \
  ldd "$extracted/usr/bin/redisdock" | tee "$work/dependencies.txt"
if grep -q 'not found' "$work/dependencies.txt"; then exit 1; fi
echo "LINUX PACKAGES PASS: $deb $appimage"
