#!/usr/bin/env python3
"""Check the signed app against the Cargo binary without comparing signature bytes."""
import argparse
import hashlib
import json
from pathlib import Path
import struct
import subprocess
import uuid

ROOT = Path(__file__).resolve().parents[1]


def inspect(path):
    data = path.read_bytes()
    assert struct.unpack_from("<I", data)[0] == 0xFEEDFACF, "Expected a thin 64-bit Mach-O binary"
    position = 32
    sections = {}
    identity = None
    for _ in range(struct.unpack_from("<I", data, 16)[0]):
        command, length = struct.unpack_from("<II", data, position)
        if command == 0x1B:  # LC_UUID
            identity = str(uuid.UUID(bytes=data[position + 8:position + 24])).upper()
        if command == 0x19:  # LC_SEGMENT_64
            segment = data[position + 8:position + 24].rstrip(b"\0").decode()
            if segment in ("__TEXT", "__DATA_CONST"):
                for index in range(struct.unpack_from("<I", data, position + 64)[0]):
                    section = position + 72 + index * 80
                    name = data[section:section + 16].rstrip(b"\0").decode()
                    size = struct.unpack_from("<Q", data, section + 40)[0]
                    offset = struct.unpack_from("<I", data, section + 48)[0]
                    if size and offset:
                        assert offset + size <= len(data)
                        sections[segment + "," + name] = {
                            "size": size,
                            "sha256": hashlib.sha256(data[offset:offset + size]).hexdigest(),
                        }
        position += length
    assert identity and sections, "Missing UUID or code/constant sections"
    return identity, sections


parser = argparse.ArgumentParser(description=__doc__)
parser.add_argument("profile", choices=("debug", "release"), nargs="?", default="release")
args = parser.parse_args()
bundle = ROOT / "target/bundle/Tiny RDM.app"
build_uuid, build_sections = inspect(ROOT / "target" / args.profile / "rust-rdm")
bundle_uuid, bundle_sections = inspect(bundle / "Contents/MacOS/rust-rdm")
assert (build_uuid, build_sections) == (bundle_uuid, bundle_sections), "Bundle differs from the selected Cargo build"
subprocess.run(["codesign", "--verify", "--deep", "--strict", str(bundle)], check=True)
report = {
    "strictSignatureVerified": True,
    "buildUUID": build_uuid,
    "bundleUUID": bundle_uuid,
    "comparedSectionCount": len(build_sections),
    "codeAndConstantSectionsIdentical": True,
    "sections": build_sections,
    "scope": "Nonempty Mach-O __TEXT/__DATA_CONST sections and UUID; whole-file bytes differ due to signing metadata.",
}
output = ROOT / "outputs/bundle-code-check.json"
output.parent.mkdir(exist_ok=True)
output.write_text(json.dumps(report, indent=2) + "\n")
print(f"BUNDLE PASS: strict signature, {len(build_sections)} sections, UUID {build_uuid}")
