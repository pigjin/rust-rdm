#!/usr/bin/env python3
"""Compare sampled dark CSV backgrounds and visible text/icon colors."""
import json
from collections import Counter
from pathlib import Path

from PIL import Image

ROOT = Path(__file__).resolve().parents[1]
fixture = json.loads((ROOT / "tests/fixtures/csv-dark-source.json").read_text())
checks = []
failures = []


def has_class(element, name):
    return name in element["classes"].split()


def contains(bounds, x, y):
    return bounds["x"] <= x < bounds["x"] + bounds["width"] and bounds["y"] <= y < bounds["y"] + bounds["height"]


for sample in fixture["records"]:
    scene = sample["name"]
    pictures = {name: Image.open(ROOT / "outputs" / name / (scene + ".png")).convert("RGB")
                for name in ("ui-reference", "ui-qa")}
    elements = sample["elements"]
    popup = next((e["bounds"] for e in elements if has_class(e, "n-base-select-menu")), None)
    measurements = []
    for index, element in enumerate(elements):
        names = [name for name in ("n-dialog", "n-input", "n-button", "n-radio-button", "n-checkbox-box",
                                    "n-card", "n-base-selection", "n-base-select-option") if has_class(element, name)]
        if not names:
            continue
        bounds = element["bounds"]
        x, y = bounds["x"] + 8, bounds["y"] + 7
        if popup and contains(popup, x, y) and names[0] != "n-base-select-option":
            continue  # The popup occludes the footer, so it cannot measure button paint.
        point = (round(x * 2), round(y * 2))
        colors = {name: picture.getpixel(point) for name, picture in pictures.items()}
        passed = max(abs(a - b) for a, b in zip(*colors.values())) <= 1
        measurement = {"element": names[0], "index": index, "point": point, "colors": colors, "passed": passed}
        measurements.append(measurement)
        if not passed:
            failures.append({"scene": scene, **measurement})

    # The primary label is black in the original dark theme.
    # Check representative full-ink pixels, without asserting glyph raster equality.
    if not popup:
        primary = [e for e in elements if has_class(e, "n-button")][-1]
        color = (27, 27, 31) if sample["buttons"][-1]["disabled"] else (0, 0, 0)
        ink = {"element": "primary label", "color": color, "pixels": {}}
        for name, picture in pictures.items():
            b = primary["bounds"]
            box = tuple(round(v * 2) for v in (b["x"] + 10, b["y"] + 5,
                                               b["x"] + b["width"] - 10, b["y"] + b["height"] - 5))
            colors = Counter(picture.crop(box).get_flattened_data())
            ink["pixels"][name] = sum(count for rgb, count in colors.items()
                                       if max(abs(a - b) for a, b in zip(rgb, color)) <= 1)
        ink["passed"] = min(ink["pixels"].values()) > 0
        measurements.append(ink)
        if not ink["passed"]:
            failures.append({"scene": scene, **ink})
    icons = []
    arrow = next((e for e in elements if has_class(e, "n-base-suffix")), None)
    if arrow:
        control = next(e for e in elements if has_class(e, "n-base-selection"))
        b = control["bounds"]
        background = pictures["ui-reference"].getpixel((round((b["x"] + 8) * 2), round((b["y"] + 7) * 2)))
        color = tuple(round(channel * 0.62 + 255 * 0.38) for channel in background)
        icons.append(("select arrow", arrow["bounds"], color))
    if sample["checkboxes"][0]["checked"]:
        checkbox = next(e for e in elements if has_class(e, "n-checkbox-box"))
        assert checkbox["vars"]["--n-check-mark-color"].strip() == "rgb(24, 24, 28)"
        icons.append(("checkbox check", checkbox["bounds"], (24, 24, 28)))
    for label, b, color in icons:
        box = tuple(round(v * 2) for v in (b["x"], b["y"], b["x"] + b["width"], b["y"] + b["height"]))
        tolerance = 2 if label == "select arrow" and popup else 1
        ink = {"element": label, "color": color, "RGBChannelTolerance": tolerance, "pixels": {}}
        for name, picture in pictures.items():
            colors = Counter(picture.crop(box).get_flattened_data())
            ink["pixels"][name] = sum(count for rgb, count in colors.items()
                                       if max(abs(a - b) for a, b in zip(rgb, color)) <= tolerance)
        ink["passed"] = min(ink["pixels"].values()) > 0
        measurements.append(ink)
        if not ink["passed"]:
            failures.append({"scene": scene, **ink})
    checks.append({"scene": scene, "measurements": measurements, "passed": all(m["passed"] for m in measurements)})

report = {"scope": "Seven dark CSV scenes: sampled modal/control/option backgrounds, primary label, arrow and checkbox ink. "
                    "Allows one RGB channel step for compositing rounding; the arrow under the popup allows two "
                    "because the GPUI popup shadow darkens it slightly more than CSS. Excludes whole-image equality, "
                    "font rasterization, shadows, borders and focus-transition timing.",
          "RGBChannelTolerance": 1, "checks": checks, "passed": not failures}
(ROOT / "outputs/csv-dark-color-check.json").write_text(json.dumps(report, indent=2) + "\n")
assert not failures, json.dumps(failures, ensure_ascii=False, indent=2)
print(f"CSV DARK COLOR PASS: {len(checks)} scenes")
