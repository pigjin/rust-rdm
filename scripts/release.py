#!/usr/bin/env python3
"""Version and artifact checks shared by local packaging and GitHub Releases."""
import argparse
import hashlib
import json
from pathlib import Path
import struct
import subprocess

ROOT = Path(__file__).resolve().parents[1]


def package_version():
    metadata = json.loads(subprocess.check_output(
        ["cargo", "metadata", "--locked", "--no-deps", "--format-version", "1"], cwd=ROOT,
    ))
    return next(p["version"] for p in metadata["packages"] if p["name"] == "redisdock")


def core_version(version):
    return version.split("+", 1)[0].split("-", 1)[0]


def deb_version(version):
    base, separator, build = version.partition("+")
    return base.replace("-", "~", 1) + separator + build


def check_tag(version, tag):
    if tag != "v" + version:
        raise ValueError(f"Tag {tag!r} must match Cargo version: v{version}")


def binary_arch(path):
    with Path(path).open("rb") as source:
        header = source.read(64)
        if header[:4] == b"\x7fELF" and header[4:6] == b"\x02\x01":
            return {62: "x86_64", 183: "arm64"}[struct.unpack_from("<H", header, 18)[0]]
        if header[:4] == b"\xcf\xfa\xed\xfe":
            return {0x1000007: "x86_64", 0x100000C: "arm64"}[struct.unpack_from("<I", header, 4)[0]]
        if header[:2] == b"MZ":
            source.seek(struct.unpack_from("<I", header, 60)[0])
            pe_header = source.read(26)
            if pe_header[:4] == b"PE\0\0" and pe_header[24:26] == b"\x0b\x02":
                return {0x8664: "x86_64", 0xAA64: "arm64"}[struct.unpack_from("<H", pe_header, 4)[0]]
    raise ValueError(f"Not a supported 64-bit executable: {path}")


def asset_names(version):
    return sorted(
        f"RedisDock-{version}-{system}-{arch}{extension}"
        for system, extension in (
            ("macos", ".dmg"), ("windows", "-setup.exe"),
            ("linux", ".AppImage"), ("linux", ".deb"),
        )
        for arch in ("x86_64", "arm64")
    )


def check_assets(directory, version):
    directory = Path(directory)
    expected = set(asset_names(version))
    actual = {p.name for p in directory.iterdir()} - {"SHA256SUMS"}
    if actual != expected:
        raise ValueError(f"Incorrect assets: missing={sorted(expected - actual)}, unexpected={sorted(actual - expected)}")
    for name in sorted(expected):
        path = directory / name
        if not path.is_file() or path.is_symlink() or path.stat().st_size == 0:
            raise ValueError(f"Empty or invalid asset: {path}")
    return [directory / name for name in sorted(expected)]


def checksum_manifest(directory, version):
    lines = []
    for path in check_assets(directory, version):
        digest = hashlib.sha256()
        with path.open("rb") as source:
            for chunk in iter(lambda: source.read(1024 * 1024), b""):
                digest.update(chunk)
        lines.append(f"{digest.hexdigest()}  {path.name}\n")
    manifest = Path(directory) / "SHA256SUMS"
    manifest.write_text("".join(lines), encoding="utf-8")
    return manifest


def check_upload(directory, version, release):
    paths = check_assets(directory, version) + [Path(directory) / "SHA256SUMS"]
    expected = {path.name: path.stat().st_size for path in paths}
    actual = {asset["name"]: asset["size"] for asset in release["assets"]}
    if release["isDraft"] is not True or release["tagName"] != "v" + version or release["name"] != "RedisDock " + version:
        raise ValueError("Expected an unpublished RedisDock release for the selected tag")
    if actual != expected or len(release["assets"]) != len(expected):
        raise ValueError(f"Uploaded assets differ from local files: {actual!r}")


def main():
    parser = argparse.ArgumentParser(description=__doc__)
    commands = parser.add_subparsers(dest="command", required=True)
    version_parser = commands.add_parser("version")
    version_parser.add_argument("--tag")
    formats = version_parser.add_mutually_exclusive_group()
    formats.add_argument("--core", action="store_true")
    formats.add_argument("--deb", action="store_true")
    version_parser.add_argument("--github-output", type=Path)
    binary_parser = commands.add_parser("binary")
    binary_parser.add_argument("path", type=Path)
    binary_parser.add_argument("arch", choices=("x86_64", "arm64"))
    for command in ("manifest", "check-upload"):
        subparser = commands.add_parser(command)
        subparser.add_argument("directory", type=Path)
        subparser.add_argument("--version", required=True)
        if command == "check-upload":
            subparser.add_argument("release", type=Path)
    args = parser.parse_args()
    if args.command == "version":
        version = package_version()
        if args.tag is not None:
            check_tag(version, args.tag)
        if args.github_output:
            with args.github_output.open("a", encoding="utf-8") as output:
                prerelease = str("-" in version.split("+", 1)[0]).lower()
                output.write(f"version={version}\ncore-version={core_version(version)}\nprerelease={prerelease}\n")
        print(core_version(version) if args.core else deb_version(version) if args.deb else version)
    elif args.command == "binary":
        actual = binary_arch(args.path)
        if actual != args.arch:
            raise ValueError(f"Incorrect binary architecture: {actual}, expected {args.arch}")
        print(f"BINARY PASS: {args.path.name} {actual}")
    elif args.command == "manifest":
        print(checksum_manifest(args.directory, args.version))
    else:
        check_upload(args.directory, args.version, json.loads(args.release.read_text(encoding="utf-8")))
        print("UPLOAD PASS: release identity and all nine asset names/sizes")


if __name__ == "__main__":
    main()
