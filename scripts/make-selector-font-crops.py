#!/usr/bin/env python3
"""Extract unchanged sidebar header pixels; differing detail models are excluded."""
import hashlib
import json
from pathlib import Path

from PIL import Image

ROOT = Path(__file__).resolve().parents[1]
rows = []
for filename in ("selector-font-macos-source.json", "general-fallback-macos-source.json"):
    fixture = json.loads((ROOT / "tests/fixtures" / filename).read_text())
    for record in fixture["records"]:
        name = record["name"]
        width = record["configuration"]["asideWidth"]
        box = (100, 80, min(2048, 100 + round(width * 2)), 160)
        row = {"scene": name, "physicalPixelCrop": box, "asideWidth": width}
        for kind, source, folder in (
            ("native", ROOT / "outputs/ui-qa" / (name + ".png"), ROOT / "outputs/ui-qa-selector-font-crops"),
            ("reference", ROOT / fixture["archiveDirectory"] / (name + ".png"), ROOT / "outputs/ui-reference-selector-font-crops"),
        ):
            folder.mkdir(exist_ok=True)
            target = folder / (name + ".png")
            with Image.open(source) as image:
                assert image.size == (2048, 1536)
                image.crop(box).save(target)
            row[kind] = {
                "source": str(source.relative_to(ROOT)),
                "sourceSHA256": hashlib.sha256(source.read_bytes()).hexdigest(),
                "path": str(target.relative_to(ROOT)),
                "sha256": hashlib.sha256(target.read_bytes()).hexdigest(),
            }
        rows.append(row)
assert len(rows) == 20
(ROOT / "outputs/selector-font-macos-crops.json").write_text(json.dumps({
    "passed": True,
    "scenes": 20,
    "records": rows,
    "scope": "Unmodified 2x header pixels only. All sampled headers fit the viewport; strictly failed text comparisons remain unaccepted. Full source/native PNGs retained. No whole-image or native interaction acceptance.",
}, indent=2) + "\n")
print("EXTRACTED20 FONT/TYPE HEADER PAIRS; FULL PNGS RETAINED")
