#!/usr/bin/env python3
"""Verify independent WK font-box samples; app rendering is checked separately."""
import hashlib
import json
import re
from pathlib import Path

ROOT = Path(__file__).resolve().parents[1]
fixture_path = ROOT / "tests/fixtures/css-font-metrics-macos-source.json"
fixture = json.loads(fixture_path.read_text())
raw_path = ROOT / fixture["rawReport"]
assert hashlib.sha256(raw_path.read_bytes()).hexdigest() == fixture["rawSHA256"]
raw = json.loads(raw_path.read_text())["records"]
report = json.loads((ROOT / fixture["diagnosticReport"]).read_text())
assert report["processPassed"] and report["recordCount"] == 49
assert report["currentAppScenarioCoverage"] is False
files = [fixture_path, raw_path, ROOT / fixture["diagnosticReport"], Path(__file__)]
for row in report["files"]:
    path = ROOT / row["path"]
    assert hashlib.sha256(path.read_bytes()).hexdigest() == row["sha256"], path
    files.append(path)

assert len(raw) == len(fixture["records"]) == 49
expected = []
for actual, pinned in zip(raw, fixture["records"]):
    assert actual["markerDidNotChangeLayout"]
    assert all(actual[key] == value for key, value in pinned.items())
    family = ".SystemUIFont" if pinned["family"] == "-apple-system" else pinned["family"]
    expected.append((family, pinned["size"], pinned["webkitFontBoxAscent"], pinned["webkitFontBoxDescent"]))
assert len({(row[0], row[1]) for row in expected}) == 49
assert sorted({row[1] for row in expected}) == fixture["pointSizes"]

backend = ROOT / "vendor/gpui-apple/src/text_system.rs"
body = backend.read_text().split("fn rdm_webkit_font_boxes_match_css_source_samples()", 1)[1]
body = body.split("let samples = [", 1)[1].split("];", 1)[0]
samples = [(family, float(size), float(ascent), float(descent))
           for family, size, ascent, descent in re.findall(
               r'\("([^"]+)", ([0-9.]+), ([0-9.]+), ([0-9.]+)\)', body)]
assert samples == expected
files.append(backend)
result = {
    "passed": True,
    "independentWKFontBoxSamples": 49,
    "originalAppInteractionStates": 0,
    "pointSizes": fixture["pointSizes"],
    "rawReport": fixture["rawReport"],
    "rustExpectedValuesMatchMeasuredCSS": True,
    "files": [{"path": str(path.relative_to(ROOT)),
               "sha256": hashlib.sha256(path.read_bytes()).hexdigest()}
              for path in sorted(set(files))],
    "scope": "Source capture, fixture and Rust expectation integrity only. These49 independent CSS samples are not49 original app interaction states; actual GPUI tests,images andnative acceptance remain separate.",
}
(ROOT / "outputs/css-font-metrics-macos-source-check.json").write_text(json.dumps(result, indent=2) + "\n")
print("CSS FONT METRICS SOURCE VERIFIED:49 independent WK samples;app render/native acceptance separate")
