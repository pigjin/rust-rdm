#!/usr/bin/env python3
"""Compare Source RGB and selection bounds, excluding glyph antialiasing."""
import json
from pathlib import Path

from PIL import Image

ROOT = Path(__file__).resolve().parents[1]
fixture = json.loads((ROOT / "tests/fixtures/csv-selection-source.json").read_text())
checks = []
for sample in fixture["records"]:
    if sample["name"].endswith(("plain", "dark")):
        continue
    bounds = sample["metrics"][0]["bounds"]
    region = tuple(round(value * 2) for value in (
        bounds["x"] - 2, bounds["y"] - 2,
        bounds["x"] + bounds["width"] + 2, bounds["y"] + bounds["height"] + 2,
    ))
    found = {}
    for folder in ("ui-reference", "ui-qa"):
        picture = Image.open(ROOT / "outputs" / folder / (sample["name"] + ".png")).convert("RGB")
        pixels = picture.load()
        points = [(x, y) for y in range(region[1], region[3]) for x in range(region[0], region[2])
                  if pixels[x, y] == (177, 213, 253)]
        assert points, (sample["name"], folder, "missing Source selection color")
        found[folder] = {
            "pixels": len(points),
            "bounds": [min(x for x, _ in points), min(y for _, y in points),
                       max(x for x, _ in points), max(y for _, y in points)],
        }
    delta = [abs(a - b) for a, b in zip(found["ui-reference"]["bounds"], found["ui-qa"]["bounds"])]
    assert max(delta) <= 2, (sample["name"], "selection bounds", delta)
    checks.append({"scene": sample["name"], "rgb": [177, 213, 253], "measurements": found,
                   "boundsDeltaInDevicePixels": delta, "passed": True})
(ROOT / "outputs/csv-selection-color-check.json").write_text(json.dumps({
    "scope": "Selection color and bounds only; excludes full-image equality, glyph antialiasing and dark mode.",
    "toleranceInDevicePixels": 2, "checks": checks,
}, indent=2) + "\n")
print(f"CSV SELECTION COLOR PASS: {len(checks)} Source selection colors and bounds")
