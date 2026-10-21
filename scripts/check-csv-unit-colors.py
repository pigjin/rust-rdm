#!/usr/bin/env python3
"""Check Source option colors and both original select SVG colors."""
import json
from collections import Counter
from pathlib import Path

from PIL import Image

ROOT = Path(__file__).resolve().parents[1]
fixture = json.loads((ROOT / "tests/fixtures/csv-unit-source.json").read_text())
checks = []
for sample in fixture["records"]:
    if not sample["menu"]:
        continue
    rows = [item for item in sample["elements"] if item["classes"].startswith("n-base-select-option ")]
    sources = {}
    for folder in ("ui-reference", "ui-qa"):
        picture = Image.open(ROOT / "outputs" / folder / (sample["name"] + ".png")).convert("RGB")
        colors = []
        for row in rows:
            area = row["bounds"]
            point = (round((area["x"] + 8) * 2), round((area["y"] + 17) * 2))
            actual = picture.getpixel(point)
            expected = (243, 243, 245) if "--pending" in row["classes"] else (255, 255, 255)
            assert actual == expected, (sample["name"], folder, "option background", point, actual, expected)
            colors.append(actual)
        graphic_counts = {}
        for label, classes, color in (("arrow", "n-base-suffix", (194, 194, 194)),
                                      ("check", "n-base-select-option__check", (211, 58, 49))):
            element = next(item for item in sample["elements"] if classes in item["classes"])
            area = element["bounds"]
            box = tuple(round(value * 2) for value in (area["x"] - 1, area["y"] - 1,
                                                       area["x"] + area["width"] + 1,
                                                       area["y"] + area["height"] + 1))
            pixels = Counter(picture.crop(box).get_flattened_data())
            graphic_counts[label] = sum(count for rgb, count in pixels.items()
                                        if max(abs(a - b) for a, b in zip(rgb, color)) <= 1)
            assert graphic_counts[label] > 0, (sample["name"], folder, "missing SVG color", label, color)
        sources[folder] = {"optionBackgrounds": colors, "SVGColorPixelsWithinOneChannelStep": graphic_counts}
    checks.append({"scene": sample["name"], "measurements": sources, "passed": True})
(ROOT / "outputs/csv-unit-color-check.json").write_text(json.dumps({
    "scope": "Exact light option background points; Source arrow/check RGB within one channel step in icon regions to allow raster rounding. Excludes whole-image equality and dark mode.",
    "SVGChannelTolerance": 1,
    "checks": checks,
}, indent=2) + "\n")
print(f"CSV UNIT COLOR PASS: {len(checks)} option/arrow/check scenes")
