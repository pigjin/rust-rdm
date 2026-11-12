"""Publish the native macOS downloads from a stable RedisDock release as a cask."""
import argparse
import json
from pathlib import Path
import re
import subprocess
import tempfile

from release import asset_names, file_sha256


def version_number(version):
    number = r"(0|[1-9][0-9]*)"
    match = re.fullmatch(rf"{number}\.{number}\.{number}(?:\+[0-9A-Za-z-]+(?:\.[0-9A-Za-z-]+)*)?", version)
    if not match:
        raise ValueError(f"Invalid stable release version: {version!r}")
    return tuple(map(int, match.groups()))


def published_version(release):
    tag = release["tagName"]
    if release["isDraft"] or release["isPrerelease"] or "-" in tag.partition("+")[0]:
        return None
    if not tag.startswith("v"):
        raise ValueError(f"Invalid release tag: {tag!r}")
    version_number(tag[1:])
    return tag[1:]


def update_cask(directory, version, cask):
    number = version_number(version)
    names = [name for name in asset_names(version) if name.endswith(".dmg")]
    if {p.name for p in directory.iterdir()} != {"SHA256SUMS", *names}:
        raise ValueError(f"Expected SHA256SUMS and both macOS packages for {version}")
    rows = [line.split("  ", 1) for line in (directory / "SHA256SUMS").read_text().splitlines()]
    hashes = {}
    for name in names:
        package = directory / name
        if not package.is_file() or package.is_symlink() or not package.stat().st_size:
            raise ValueError(f"Invalid DMG: {name}")
        digest = file_sha256(package)
        if [row[0] for row in rows if row[1:] == [name]] != [digest]:
            raise ValueError(f"Missing, duplicate or incorrect SHA-256 for {name}")
        hashes[name] = digest
    arm = hashes[f"RedisDock-{version}-macos-arm64.dmg"]
    intel = hashes[f"RedisDock-{version}-macos-x86_64.dmg"]

    if cask.exists():
        current = cask.read_text()
        match = re.search(r'^\s*version\s+"([^"]+)"\s*$', current, re.MULTILINE)
        if not match:
            raise ValueError(f"Missing version in {cask}")
        previous = match[1]
        if previous == version:
            checksum = re.search(r'\bsha256\s+arm:\s*"([0-9a-f]{64})",\s*intel:\s*"([0-9a-f]{64})"', current)
            if not checksum or checksum.groups() != (arm, intel):
                raise ValueError(f"Published SHA-256 changed for {version}")
            print(f"Tap already contains {version}")
            return True
        # SemVer build metadata does not make a release newer.
        if number <= version_number(previous):
            print(f"Keeping {previous}, skipping {version}")
            return False

    cask.parent.mkdir(parents=True, exist_ok=True)
    cask.write_text(f'''cask "redisdock" do
  arch arm: "arm64", intel: "x86_64"

  version "{version}"
  sha256 arm:   "{arm}",
         intel: "{intel}"

  url "https://github.com/pigjin/rust-rdm/releases/download/v#{{version}}/RedisDock-#{{version}}-macos-#{{arch}}.dmg"
  name "RedisDock"
  desc "Desktop client for Redis"
  homepage "https://github.com/pigjin/rust-rdm"

  depends_on :macos

  app "RedisDock.app"
end
''')
    return True


def main():
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("cask", type=Path)
    parser.add_argument("--tag", default="", help="Defaults to the latest published release")
    parser.add_argument("--github-output", type=Path)
    args = parser.parse_args()
    repo = ["--repo", "pigjin/rust-rdm"]
    tags = [args.tag] if args.tag else []
    release = json.loads(subprocess.check_output(
        ["gh", "release", "view", *repo, "--json", "tagName,isDraft,isPrerelease", "--", *tags],
    ))
    version = published_version(release)
    if version is None:
        print("Skipping draft or prerelease")
        return
    with tempfile.TemporaryDirectory() as temporary:
        subprocess.run(["gh", "release", "download", *repo, "--dir", temporary, "--pattern", "SHA256SUMS",
                        "--pattern", "*.dmg", "--", "v" + version], check=True)
        if update_cask(Path(temporary), version, args.cask) and args.github_output:
            with args.github_output.open("a") as output:
                output.write(f"version={version}\n")


if __name__ == "__main__":
    main()
