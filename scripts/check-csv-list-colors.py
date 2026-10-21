#!/usr/bin/env python3
"""Compare virtual-list selection paint and the Source scrollbar hover color."""
import json
from pathlib import Path

from PIL import Image

ROOT = Path(__file__).resolve().parents[1]
fixture = json.loads((ROOT / "tests/fixtures/csv-list-source.json").read_text())
checks = []
for sample in fixture["records"]:
    if not sample["selection"]:
        continue
    bounds = sample["bounds"]
    color = (95, 131, 172) if "dark" in sample["name"] else (177, 213, 253)
    measurements = {}
    for folder in ("ui-reference", "ui-qa"):
        picture = Image.open(ROOT / "outputs" / folder / (sample["name"] + ".png")).convert("RGB")
        pixels = picture.load()
        points = [(x, y) for y in range(round(bounds["y"] * 2), round((bounds["y"] + bounds["height"]) * 2))
                  for x in range(round(bounds["x"] * 2), round((bounds["x"] + bounds["width"]) * 2))
                  if max(abs(a - b) for a, b in zip(pixels[x, y], color)) <= 1]
        assert points, (sample["name"], folder, "missing selection color")
        measurements[folder] = {"pixels": len(points), "bounds": [min(x for x, _ in points), min(y for _, y in points),
                                                                    max(x for x, _ in points), max(y for _, y in points)]}
    delta = [abs(a - b) for a, b in zip(measurements["ui-reference"]["bounds"], measurements["ui-qa"]["bounds"])]
    assert max(delta) <= 2, (sample["name"], delta)
    checks.append({"scene": sample["name"], "rgb": color, "measurements": measurements,
                   "boundsDeltaInDevicePixels": delta, "passed": True})
sample = next(s for s in fixture["records"] if s["name"] == "csv-list-hover")
thumb = sample["scrollbars"][1]["bounds"]
point = (round((thumb["x"] + 4) * 2), round((thumb["y"] + 6) * 2))
colors = {folder: Image.open(ROOT / "outputs" / folder / (sample["name"] + ".png")).convert("RGB").getpixel(point)
          for folder in ("ui-reference", "ui-qa")}
assert all(color == (150, 150, 151) for color in colors.values()), colors
checks.append({"scene": sample["name"], "point": point, "colors": colors, "passed": True})
(ROOT / "outputs/csv-list-color-check.json").write_text(json.dumps({
    "scope": "Four selection colors/bounds and one hovered thumb background point. Selection RGB allows one channel step; "
             "bounds allow two device pixels. Excludes whole-image equality, font rasterization and scrollbar transitions.",
    "checks": checks, "passed": True,
}, indent=2) + "\n")
print(f"CSV LIST COLOR PASS: {len(checks)} selection/scrollbar scenes")
