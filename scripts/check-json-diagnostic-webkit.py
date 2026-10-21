#!/usr/bin/env python3
"""Check two bounded marker extents against one original WKWebView still.

The source probe finishes finite animations and supplies timer-based rAF in its
own hidden window. It does not verify animation timing, native interaction or
whole-page equality. Chromium comparison failures are not changed here.
"""
import hashlib
import json
from datetime import datetime, timezone
from pathlib import Path
import subprocess

from PIL import Image

ROOT = Path(__file__).resolve().parents[1]
OUT = ROOT / "outputs"
SOURCE_COPY = Path("/private/tmp/rust-rdm-source-qa")
ORIGINAL = Path("/Users/zlsj/Projects/tiny-rdm")
COMMIT = "4ddb94a3c55f3401dbc66870f0d21cf0d6203cbc"


def sha(path):
    return hashlib.sha256(path.read_bytes()).hexdigest()


def load(path):
    return json.loads((OUT / path).read_text())


source = load("json-diagnostic-webkit-source.json")
state = json.loads(source["beforeSnapshot"])
assert state["sourceCommit"] == COMMIT
assert state["sourceURL"] == "http://127.0.0.1:5173/"
assert state["viewport"] == [1024, 768] and source["snapshotSize"] == [2048, 1536]
assert state["editorValue"] == '{"🙂":}' and state["language"] == "json"
assert len(state["markers"]) == 1
marker = state["markers"][0]
assert (marker["code"], marker["startColumn"], marker["endColumn"]) == ("516", 7, 8)
assert len(state["decorations"]) == 1
assert state["decorations"][0]["box"]["width"] == 9
assert state["raf"]["mode"] == "controlled-timer-layout-shim"
assert all(row["opacity"] == "1" for row in state["spinContent"])
assert state["spinContent"] and any(row.get("finished") for row in state["animations"])
assert not state["fullVisualParityAccepted"] and not state["nativeWindowInteractionAccepted"]

# Recheck the actual original component files, rather than trusting the commit
# string supplied by the probe. These are the same 13 files used by the earlier
# source-evidence verifier; none is edited by this probe.
existing = load("entry-diagnostics-source-check.json")
assert existing["passed"] and existing["sourceCommit"] == COMMIT
frontend = []
for row in existing["frontendFiles"]:
    current = SOURCE_COPY / row["path"]
    original = subprocess.run(
        ["git", "show", COMMIT + ":frontend/" + row["path"]],
        cwd=ORIGINAL, check=True, capture_output=True,
    ).stdout
    assert current.read_bytes() == original, row["path"]
    assert sha(current) == row["sha256"], row["path"]
    frontend.append({"path": row["path"], "sha256": sha(current), "unchanged": True})
assert len(frontend) == 13
assert json.loads((SOURCE_COPY / "node_modules/monaco-editor/package.json").read_text())["version"] == "0.47.0"
monaco_files = []
for row in existing["installedMonacoFiles"]:
    current = SOURCE_COPY / "node_modules/monaco-editor" / row["path"]
    assert sha(current) == row["sha256"], row["path"]
    monaco_files.append(row)

image_paths = [OUT / "ui-reference-webkit/entry-diagnostic-unicode.png",
               OUT / "ui-qa/entry-diagnostic-unicode.png"]
images = [Image.open(path).convert("RGB") for path in image_paths]
assert images[0].size == images[1].size == (2048, 1536)
assert sum(pixel != (255, 255, 255) for pixel in images[0].get_flattened_data()) > 500000


def mask(image, bounds, color):
    x1, y1, x2, y2 = bounds
    points = [(x, y) for y in range(y1, y2) for x in range(x1, x2)
              if max(abs(a - b) for a, b in zip(image.getpixel((x, y)), color)) <= 1]
    return {"pixels": len(points), "bounds":
            ([min(p[i] for p in points) for i in (0, 1)]
             + [max(p[i] for p in points) for i in (0, 1)]) if points else None}


checks = []
d = state["decorations"][0]["box"]
e = state["editorBox"]
samples = [
    ("error underline", (229, 20, 0),
     [d["x"] - 2, d["y"] + d["height"] - 4,
      d["x"] + d["width"] + 2, d["y"] + d["height"] + 1]),
    ("overview ruler", (255, 89, 89),
     [e["x"] + e["width"] - 5, e["y"] + 2,
      e["x"] + e["width"], e["y"] + e["height"]]),
]
for name, color, logical_bounds in samples:
    bounds = [round(value * 2) for value in logical_bounds]
    masks = [mask(image, bounds, color) for image in images]
    delta = (max(abs(a - b) for a, b in zip(masks[0]["bounds"], masks[1]["bounds"]))
             if all(m["bounds"] for m in masks) else None)
    checks.append({"scene": "entry-diagnostic-unicode", "sample": name,
                   "color": color, "sampleBounds": bounds, "masks": masks,
                   "maxPhysicalPixelDelta": delta, "allowedPhysicalPixelDelta": 2,
                   "passed": delta is not None and delta <= 2})

# Preserve the unsuccessful serialization approaches as rejected evidence.
rejected = []
for name in ("static-unicode", "static-computed"):
    r = load("webkit-rejected-probes/" + name + ".json")
    layout = json.loads(r["staticLayout"])
    original = r["editorBox"]
    actual = layout["editor"]["box"]
    p = OUT / "webkit-rejected-probes" / (name + ".png")
    picture = Image.open(p).convert("RGB")
    nonwhite = sum(pixel != (255, 255, 255) for pixel in picture.get_flattened_data())
    item = {"probe": name, "imageSha256": sha(p),
            "metadataSha256": sha(OUT / "webkit-rejected-probes" / (name + ".json")),
            "javascriptSha256": sha(OUT / "webkit-rejected-probes" / (name + ".js")),
            "swiftSha256": sha(OUT / "webkit-rejected-probes" / (name + ".swift")),
            "originalEditor": original,
            "serializedEditor": actual, "nonwhitePixels": nonwhite,
            "acceptedAsReference": False}
    if name == "static-unicode":
        assert actual != original
        item["reason"] = "Serializing CSS alone changed layout; editor shifted left 1px and up 2px."
    else:
        assert nonwhite == 0
        actual_boxes = {row["id"]: row["box"] for row in layout["instrumentedBoxes"]}
        assert len(r["originalBoxes"]) == 726
        assert all(max(abs(value - actual_boxes[row["id"]][key])
                       for key, value in row["box"].items()) <= 1 / 64
                   for row in r["originalBoxes"])
        item["matchingVisibleBoxes"] = 726
        item["reason"] = "Computed styles preserved geometry but also retained the paused transparent startup state."
    rejected.append(item)

chromium_colors = load("json-diagnostics-color-check.json")
chromium_selection = load("json-diagnostic-selection-color-check.json")
assert not chromium_colors["passed"] and not chromium_selection["passed"]
files = [*image_paths, OUT / "json-diagnostic-webkit-source.json",
         *(OUT / "source-probes" / name for name in
           ("json-diagnostic-webkit-source.swift", "json-diagnostic-webkit-source.js", "json-diagnostic-webkit-raf.js")),
         ROOT / "scripts/check-json-diagnostic-webkit.py"]
report = {
    "checkedAt": datetime.now(timezone.utc).isoformat(),
    "referenceEngine": "macOS WKWebView",
    "sourceCommit": COMMIT, "frontendFiles": frontend, "monacoVersion": "0.47.0",
    "installedMonacoFiles": monaco_files,
    "scope": "One original source-page still; two bounded marker color/extent checks at DPR 2, unchanged two-physical-pixel tolerance. Controlled local fixture, timer-based rAF, public Monaco layout/render and explicit completion of finite CSS animations. No DOM serialization in the accepted source image. No full-image, glyph/antialiasing, animation timing, actual native-window, selection, scrolling or other-platform acceptance.",
    "sourcePNGRenderedDirectlyFromOriginalPage": True,
    "sourcePNGCreatedByStaticDOMSerialization": False,
    "boundedOriginalSceneCount": 1,
    "finiteAnimationsFinished": sum(bool(row.get("finished")) for row in state["animations"]),
    "startupContentOpacity": [row["opacity"] for row in state["spinContent"]],
    "checks": checks,
    "rejectedStaticCopyProbes": rejected,
    "chromiumComparisonsRemainFailed": True,
    "files": [{"path": str(path.relative_to(ROOT)), "sha256": sha(path)} for path in files],
    "fullVisualParityComplete": False, "nativeWindowInteractionAccepted": False,
    "passed": all(check["passed"] for check in checks),
}
(OUT / "json-diagnostic-webkit-check.json").write_text(json.dumps(report, indent=2) + "\n")
assert report["passed"], "WKWebView marker extent comparison failed"
print("WKWEBVIEW ORIGINAL SCENE PASS: two bounded marker checks, deltas "
      + ", ".join(str(check["maxPhysicalPixelDelta"]) for check in checks)
      + " physical pixels; full-image parity unverified")
