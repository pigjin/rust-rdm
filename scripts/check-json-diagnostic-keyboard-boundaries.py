#!/usr/bin/env python3
"""Verify original macOS diagnostic keyboard observations; GPUI is separate."""
import hashlib
import json
import subprocess
from datetime import datetime, timezone
from pathlib import Path

ROOT = Path(__file__).resolve().parents[1]
OUT = ROOT / "outputs"
SOURCE = Path("/private/tmp/rust-rdm-source-qa")
ORIGINAL = Path("/Users/zlsj/Projects/tiny-rdm")
COMMIT = "4ddb94a3c55f3401dbc66870f0d21cf0d6203cbc"


def read(name):
    return json.loads((ROOT / name).read_text())


def sha(path):
    return hashlib.sha256(path.read_bytes()).hexdigest()


def key(name):
    if name in ("cmd-a", "shift-right", "alt-shift-right", "shift-left", "cmd-shift-left", "alt-shift-left"):
        return name
    for prefix, action in (("word-back-", "alt-shift-left"), ("word-forward-", "alt-shift-right"),
                           ("back-", "shift-left"), ("forward-", "shift-right")):
        if name.startswith(prefix):
            return action
    assert name == "after-local-AppKit-drag", name
    return None


def project(raw):
    records = []
    for row in raw["records"][2:]:
        if row["name"] == "cmd-s":
            break
        state = row["state"]
        record = {name: state[name] for name in (
            "selected", "selectionType", "anchorOffset", "focusOffset", "anchorText", "focusText",
            "editorHasTextFocus", "editorHasWidgetFocus", "activeClass",
        )}
        record.update(name=row["name"], key=key(row["name"]))
        records.append(record)
    return records


fixture_name = "tests/fixtures/json-diagnostic-keyboard-boundaries-source.json"
fixture = read(fixture_name)
assert fixture["sourceCommit"] == COMMIT and fixture["modelValue"] == "/*"
assert fixture["message"] == "中文🙂  alpha\nsecond\tline" and fixture["suffix"] == "json(777)"
files = [fixture_name, "scripts/check-json-diagnostic-keyboard-boundaries.py"]
counts = {}
for name, raw_name, expected in (
    ("hoverRetreat", "json-diagnostic-keyboard-boundaries-raw.json", 51),
    ("hoverLinesAndWords", "json-diagnostic-keyboard-word-boundaries-raw.json", 43),
    ("navigation", "json-diagnostic-keyboard-navigation-raw.json", 6),
):
    raw_path = "outputs/" + raw_name
    raw = read(raw_path)
    assert project(raw) == fixture[name] and len(fixture[name]) == expected
    counts[name] = expected
    for row in raw["records"][2:2 + expected]:
        state = row["state"]
        assert state["value"] == fixture["modelValue"] and state["entryVisible"]
        assert not state["editorHasTextFocus"] and state["editorHasWidgetFocus"]
        assert all(event["trusted"] for event in state["events"] + state["pointerEvents"])
        assert not state["saveCalls"] and not state["requests"]
        state["selected"].encode("utf-8")  # No split surrogate in a copied range.
        assert len(state["markers"]) == 1 and state["markers"][0]["code"] == "777"
        if key(row["name"]):
            event = state["events"][-1]
            command = key(row["name"])
            expected_key = "a" if command == "cmd-a" else "ArrowLeft" if command.endswith("left") else "ArrowRight"
            assert event["key"] == expected_key and event["shift"] == ("shift" in command)
            assert event["alt"] == command.startswith("alt-") and event["meta"] == command.startswith("cmd-")
    assert [event["type"] for event in raw["records"][2]["state"]["pointerEvents"]] == ["mousedown", "mouseup"]
    files.append(raw_path)

retreat = {row["name"]: row for row in fixture["hoverRetreat"]}
assert retreat["alt-shift-left"]["selected"] == "中文🙂  "
assert [retreat["back-" + str(i)]["selected"] for i in range(1, 6)] == ["中文🙂 ", "中文🙂", "中文", "中", ""]
assert all(not row["selected"] for row in fixture["hoverRetreat"]
           if row["name"].startswith(("forward-", "word-back-")))
forward = {row["name"]: row for row in fixture["hoverLinesAndWords"]}
assert forward["forward-8"]["selected"] == "中文🙂  alpha\n"
assert forward["forward-15"]["selected"] == "中文🙂  alpha\nsecond\t"
assert forward["forward-28"]["selected"] == fixture["message"] + fixture["suffix"]
assert [forward["word-back-" + str(i)]["selected"] for i in range(1, 6)] == [
    "中文🙂  alpha\nsecond\tlinejson(", "中文🙂  alpha\nsecond\t", "中文🙂  alpha\n", "中文🙂  ", "",
]
assert all(not row["selected"] for row in fixture["hoverLinesAndWords"] if row["name"].startswith("word-forward-"))

excluded = read(fixture["excludedFooterProbe"]["raw"])
footer = next(row["state"] for row in excluded["records"] if row["name"] == "forward-29")
assert footer["selected"] == fixture["message"] + fixture["suffix"] + "\n"
assert footer["focusText"] == "No quick fixes available"
files.append(fixture["excludedFooterProbe"]["raw"])
for name in ("boundaries", "forward-boundaries", "word-boundaries"):
    files.append("outputs/source-probes/json-diagnostic-keyboard-" + name + ".swift")
files.append("outputs/source-probes/json-diagnostic-keyboard-webkit.js")

earlier = read("outputs/json-diagnostic-keyboard-source-check.json")
assert earlier["passed"] and earlier["sourceCommit"] == COMMIT
for row in earlier["files"]:
    assert sha(ROOT / row["path"]) == row["sha256"], row["path"]
for row in earlier["frontendFiles"]:
    original = subprocess.run(["git", "show", COMMIT + ":frontend/" + row["path"]],
                              cwd=ORIGINAL, check=True, capture_output=True).stdout
    assert (SOURCE / row["path"]).read_bytes() == original
    assert sha(SOURCE / row["path"]) == row["sha256"]
assert len(earlier["frontendFiles"]) == 13
for row in earlier["installedMonacoFiles"]:
    assert sha(SOURCE / "node_modules/monaco-editor" / row["path"]) == row["sha256"]
assert len(earlier["installedMonacoFiles"]) == 16

report = {
    "checkedAt": datetime.now(timezone.utc).isoformat(), "passed": True,
    "sourceCommit": COMMIT, "scope": fixture["scope"], "sourceStateCounts": counts,
    "sourceStateTotal": sum(counts.values()),
    "frontendFilesVerified": 13, "installedMonacoFilesVerified": 16,
    "originalKeyboardReportSha256": sha(OUT / "json-diagnostic-keyboard-source-check.json"),
    "collapsedSelectionStopsExtending": True, "hardLineTabAndCodeRangePreserved": True,
    "chineseWordBoundaryObserved": True, "beyondMessageFooterExplicitlyExcludedFromGPUICoverage": True,
    "globalInputOrClipboardOperationsUsed": False, "gpuiAcceptanceClaimed": False,
    "originalWailsNativeWindowAcceptanceClaimed": False,
    "files": [{"path": name, "sha256": sha(ROOT / name)} for name in files],
}
(OUT / "json-diagnostic-keyboard-boundaries-source-check.json").write_text(json.dumps(report, indent=2) + "\n")
print("ORIGINAL KEYBOARD BOUNDARIES PASS: 100 states, trusted local events, collapse/newline/tab/code/word boundaries; footer and GPUI acceptance separate")
