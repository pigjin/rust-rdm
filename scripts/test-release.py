#!/usr/bin/env python3
"""Small regression checks for the version and publication gates; no GitHub writes."""
import json
from pathlib import Path
import struct
import subprocess
import sys
import tempfile
import unittest
from unittest.mock import patch

from homebrew import published_version, update_cask
from release import (
    asset_names, binary_arch, check_assets, check_tag, check_upload,
    checksum_manifest, core_version, deb_version, file_sha256, main, package_version,
)


class ReleaseTests(unittest.TestCase):
    def test_homebrew_release_channels(self):
        release = {"tagName": "v0.1.7+build-2", "isDraft": False, "isPrerelease": False}
        self.assertEqual(published_version(release), "0.1.7+build-2")
        for field, value in (("isDraft", True), ("isPrerelease", True), ("tagName", "v0.1.8-rc.1")):
            self.assertIsNone(published_version(dict(release, **{field: value})))
        with self.assertRaises(ValueError):
            published_version(dict(release, tagName="v0.1.7/other"))

    def test_homebrew_publish_replay_and_corrupted_downloads(self):
        with tempfile.TemporaryDirectory() as temporary:
            root = Path(temporary)
            cask = root / "tap/Casks/redisdock.rb"

            def downloads(version):
                directory = root / version
                directory.mkdir()
                rows = []
                for name in asset_names(version):
                    if name.endswith(".dmg"):
                        package = directory / name
                        package.write_bytes(name.encode())
                        rows.append(f"{file_sha256(package)}  {name}\n")
                (directory / "SHA256SUMS").write_text("".join(rows))
                return directory

            current = downloads("0.1.10")
            old = downloads("0.1.9")
            self.assertTrue(update_cask(old, "0.1.9", cask))
            self.assertTrue(update_cask(current, "0.1.10", cask))
            published = cask.read_text()
            arm = current / "RedisDock-0.1.10-macos-arm64.dmg"
            intel = current / "RedisDock-0.1.10-macos-x86_64.dmg"
            self.assertIn(f'sha256 arm:   "{file_sha256(arm)}"', published)
            self.assertIn(f'intel: "{file_sha256(intel)}"', published)
            self.assertTrue(update_cask(current, "0.1.10", cask))
            self.assertFalse(update_cask(old, "0.1.9", cask))
            self.assertEqual(cask.read_text(), published)

            manifest = current / "SHA256SUMS"
            original = manifest.read_text()
            manifest.write_text(original + original)
            with self.assertRaisesRegex(ValueError, "duplicate"):
                update_cask(current, "0.1.10", cask)
            manifest.write_text(original)
            old_hash = file_sha256(arm)
            arm.write_bytes(b"modified DMG")
            with self.assertRaisesRegex(ValueError, "incorrect SHA-256"):
                update_cask(current, "0.1.10", cask)
            manifest.write_text(original.replace(old_hash, file_sha256(arm)))
            with self.assertRaisesRegex(ValueError, "SHA-256 changed"):
                update_cask(current, "0.1.10", cask)
            intel.unlink()
            with self.assertRaisesRegex(ValueError, "both macOS packages"):
                update_cask(current, "0.1.10", cask)
            self.assertEqual(cask.read_text(), published)

    def test_versions_and_tag_mismatch(self):
        for version, core, deb in (("0.1.0", "0.1.0", "0.1.0"), ("1.2.3-rc.1+build-2", "1.2.3", "1.2.3~rc.1+build-2"), ("1.2.3+build-2", "1.2.3", "1.2.3+build-2")):
            with self.subTest(version=version):
                check_tag(version, "v" + version)
                self.assertEqual(core_version(version), core)
                self.assertEqual(deb_version(version), deb)
                for tag in (version, "v9.9.9", "v" + version + "-wrong"):
                    with self.assertRaises(ValueError):
                        check_tag(version, tag)

    def test_version_cli_fails_before_emitting_outputs(self):
        script = Path(__file__).with_name("release.py")
        with tempfile.TemporaryDirectory() as temporary:
            output = Path(temporary) / "github-output"
            result = subprocess.run([sys.executable, str(script), "version", "--tag", "invalid", "--github-output", str(output)], capture_output=True, text=True)
            self.assertNotEqual(result.returncode, 0)
            self.assertFalse(output.exists())
            version = package_version()
            subprocess.run([sys.executable, str(script), "version", "--tag", "v" + version, "--github-output", str(output)], check=True, capture_output=True)
            self.assertIn("version=" + version + "\n", output.read_text())
            self.assertRegex(output.read_text(), r"(?m)^prerelease=(true|false)$")
            for candidate, prerelease in (("1.2.3-rc.1+build-2", "true"), ("1.2.3+build-2", "false")):
                result_file = Path(temporary) / candidate
                arguments = [str(script), "version", "--tag", "v" + candidate, "--github-output", str(result_file)]
                with patch("release.package_version", return_value=candidate), patch("sys.argv", arguments), patch("sys.stdout"):
                    main()
                self.assertIn(f"prerelease={prerelease}\n", result_file.read_text())

    def test_native_binary_headers(self):
        with tempfile.TemporaryDirectory() as temporary:
            binary = Path(temporary) / "binary"
            for arch, elf, macho, pe in (("x86_64", 62, 0x1000007, 0x8664), ("arm64", 183, 0x100000C, 0xAA64)):
                header = bytearray(128)
                header[:6] = b"\x7fELF\x02\x01"
                struct.pack_into("<H", header, 18, elf)
                binary.write_bytes(header)
                self.assertEqual(binary_arch(binary), arch)
                header[:4] = b"\xcf\xfa\xed\xfe"
                struct.pack_into("<I", header, 4, macho)
                binary.write_bytes(header)
                self.assertEqual(binary_arch(binary), arch)
                header[:2] = b"MZ"
                struct.pack_into("<I", header, 60, 64)
                header[64:68] = b"PE\0\0"
                struct.pack_into("<H", header, 68, pe)
                struct.pack_into("<H", header, 88, 0x20B)
                binary.write_bytes(header)
                self.assertEqual(binary_arch(binary), arch)
                struct.pack_into("<H", header, 88, 0x10B)
                binary.write_bytes(header)
                with self.assertRaises(ValueError):
                    binary_arch(binary)
            binary.write_bytes(b"not an executable")
            with self.assertRaises(ValueError):
                binary_arch(binary)

    def test_incomplete_empty_and_extra_assets_fail(self):
        with tempfile.TemporaryDirectory() as temporary:
            root = Path(temporary)
            names = asset_names("0.1.0")
            self.assertEqual(len(set(names)), 8)
            for name in names[:-1]:
                (root / name).write_bytes(b"abc")
            with self.assertRaisesRegex(ValueError, "missing="):
                check_assets(root, "0.1.0")
            (root / names[-1]).touch()
            with self.assertRaisesRegex(ValueError, "Empty"):
                check_assets(root, "0.1.0")
            (root / names[-1]).write_bytes(b"abc")
            (root / "unexpected.zip").touch()
            with self.assertRaisesRegex(ValueError, "unexpected="):
                check_assets(root, "0.1.0")

    def test_manifest_and_upload_gates(self):
        with tempfile.TemporaryDirectory() as temporary:
            root = Path(temporary)
            for name in asset_names("0.1.0"):
                (root / name).write_bytes(b"abc")
            manifest = checksum_manifest(root, "0.1.0")
            self.assertEqual(len(manifest.read_text().splitlines()), 8)
            self.assertTrue(all(line.startswith("ba7816bf8f01cfea414140de5dae2223b00361a396177a9cb410ff61f20015ad  ") for line in manifest.read_text().splitlines()))
            release = {
                "isDraft": True, "tagName": "v0.1.0", "name": "RedisDock 0.1.0",
                "assets": [{"name": p.name, "size": p.stat().st_size} for p in root.iterdir()],
            }
            check_upload(root, "0.1.0", release)
            for field, value in (("isDraft", False), ("isDraft", "true"), ("tagName", "v9.9.9"), ("name", "wrong"), ("assets", release["assets"][:-1])):
                with self.subTest(field=field), self.assertRaises(ValueError):
                    check_upload(root, "0.1.0", dict(release, **{field: value}))
            damaged = json.loads(json.dumps(release))
            damaged["assets"][0]["size"] += 1
            with self.assertRaises(ValueError):
                check_upload(root, "0.1.0", damaged)


if __name__ == "__main__":
    unittest.main()
