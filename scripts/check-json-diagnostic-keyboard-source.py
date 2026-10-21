#!/usr/bin/env python3
"""Verify bounded original-engine key behavior, without claiming GPUI parity."""
import hashlib
import json
from datetime import datetime, timezone
from pathlib import Path
import subprocess

ROOT = Path(__file__).resolve().parents[1]
OUT = ROOT / "outputs"
SOURCE = Path("/private/tmp/rust-rdm-source-qa")
ORIGINAL = Path("/Users/zlsj/Projects/tiny-rdm")
COMMIT = "4ddb94a3c55f3401dbc66870f0d21cf0d6203cbc"


def sha(path):
    return hashlib.sha256(path.read_bytes()).hexdigest()


def read(path):
    return json.loads((ROOT / path).read_text())


fixture = read("tests/fixtures/json-diagnostic-keyboard-source.json")
assert fixture["sourceCommit"] == COMMIT
wk = read("outputs/json-diagnostic-keyboard-webkit-raw.json")
chrome = read("outputs/json-diagnostic-keyboard-chromium-raw.json")
wk_records = [dict(name=row["name"], **row["state"]) for row in wk["records"]
              if row["name"] == "after-local-AppKit-drag" or row["name"] in
              ("cmd-a", "shift-right", "alt-shift-right", "shift-left", "cmd-shift-left", "cmd-s")]
assert wk_records == fixture["wkWebView"]["records"]
assert chrome["records"] == fixture["chromium"]["records"]
assert len(wk_records) == len(chrome["records"]) == 7
expected = ["中文🙂", "中文🙂", "中文🙂 ", "中文🙂  alpha", "中文🙂  alph", "中文🙂  alph", ""]
assert [row["selected"] for row in wk_records] == expected
assert [row["selected"] for row in chrome["records"]] == expected
for records in (wk_records, chrome["records"]):
    for row in records[:-1]:
        assert not row["editorHasTextFocus"] and row["editorHasWidgetFocus"]
        assert row["entryVisible"]
    assert len(records[-1]["saveCalls"]) == 1
    assert not records[-1]["saveCalls"][0]["editorHasTextFocus"]
    assert not records[-1]["entryVisible"]
    assert records[1]["events"][-1]["finalPrevented"]  # Command+A is intercepted.
    assert all(event["trusted"] for event in records[-1]["events"])
    command_events = [event for event in records[-1]["events"]
                      if event["key"] not in ("Meta", "Shift", "Alt")]
    assert [event["key"] for event in command_events] == ["a", "ArrowRight", "ArrowRight", "ArrowLeft", "ArrowLeft", "s"]
    assert all(event["key"] != "c" for event in records[-1]["events"])
assert len(wk_records[0]["pointerEvents"]) == 2
assert all(event["trusted"] for event in wk_records[0]["pointerEvents"])
assert [event["type"] for event in wk_records[0]["pointerEvents"]] == ["mousedown", "mouseup"]
assert len(wk_records[-1]["requests"]) == len(chrome["requests"]) == 1
request = wk_records[-1]["requests"][0]
assert request["url"] == "/api/browser/set-hash-value" and request["method"] == "POST"
assert json.loads(request["body"]) == fixture["expectedSavedRequest"] == chrome["requests"][0]["body"]
assert fixture["expectedSavedRequest"]["value"] == fixture["modelValue"] == "/*"

earlier = read("outputs/entry-diagnostics-source-check.json")
assert earlier["passed"] and earlier["sourceCommit"] == COMMIT
frontend = []
for row in earlier["frontendFiles"]:
    current = SOURCE / row["path"]
    original = subprocess.run(["git", "show", COMMIT + ":frontend/" + row["path"]],
                              cwd=ORIGINAL, check=True, capture_output=True).stdout
    assert current.read_bytes() == original and sha(current) == row["sha256"], row["path"]
    frontend.append(row)
assert len(frontend) == 13
for row in earlier["installedMonacoFiles"]:
    assert sha(SOURCE / "node_modules/monaco-editor" / row["path"]) == row["sha256"], row["path"]

names = ["tests/fixtures/json-diagnostic-keyboard-source.json",
         "outputs/json-diagnostic-keyboard-webkit-raw.json",
         "outputs/json-diagnostic-keyboard-chromium-raw.json",
         "outputs/source-probes/json-diagnostic-keyboard-webkit.swift",
         "outputs/source-probes/json-diagnostic-keyboard-webkit.js",
         "outputs/source-probes/json-diagnostic-keyboard-chromium.mjs",
         "scripts/check-json-diagnostic-keyboard-source.py"]
report = {
    "checkedAt": datetime.now(timezone.utc).isoformat(), "sourceCommit": COMMIT,
    "scope": fixture["scope"], "frontendFiles": frontend,
    "installedMonacoFiles": earlier["installedMonacoFiles"],
    "wkWebViewStates": 7, "chromiumStates": 7,
    "trustedWKWebViewMouseDownAndUpObserved": True,
    "keysObservedInEachEngine": 6,
    "rawKeyDownEventCounts": {"wkWebView": len(wk_records[-1]["events"]),
                              "chromium": len(chrome["records"][-1]["events"])},
    "matchingKeyboardSelectionStrings": 7,
    "saveFromFocusedMessageObservedInBothEngines": True,
    "sameSaveRequestObservedInBothEngines": True,
    "saveRequest": fixture["expectedSavedRequest"],
    "clipboardOperationsExecuted": False, "globalInputUsed": False,
    "gpuiKeyboardExpansionAcceptanceClaimed": False,
    "gpuiSaveAcceptanceClaimed": False,
    "originalWailsNativeWindowAcceptanceClaimed": False,
    "files": [{"path": name, "sha256": sha(ROOT / name)} for name in names],
    "passed": True,
}
(OUT / "json-diagnostic-keyboard-source-check.json").write_text(json.dumps(report, indent=2) + "\n")
print("ORIGINAL KEYBOARD SOURCE PASS: seven states per engine, six trusted keys and one identical save request; GPUI acceptance separate")
