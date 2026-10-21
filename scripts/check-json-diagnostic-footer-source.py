#!/usr/bin/env python3
"""Check pinned original source and trusted footer-selection captures; no UI automation."""
import hashlib
import json
import subprocess
from pathlib import Path
from datetime import datetime, timezone

ROOT = Path(__file__).resolve().parents[1]
SOURCE = Path('/private/tmp/rust-rdm-footer-source-qa')
ORIGINAL = Path('/Users/zlsj/Projects/tiny-rdm')
COMMIT = '4ddb94a3c55f3401dbc66870f0d21cf0d6203cbc'
CAPTURES = ROOT / 'outputs/source-probes/diagnostic-footer'
sha = lambda path: hashlib.sha256(path.read_bytes()).hexdigest()
assert (SOURCE / 'RDM-SOURCE-QA').read_text() == f'tiny-rdm-source-{COMMIT}\n'
prior = json.loads((ROOT / 'outputs/entry-diagnostics-source-check.json').read_text())
frontend = []
for row in prior['frontendFiles']:
    path = SOURCE / 'frontend' / row['path']
    pinned = subprocess.check_output(['git', '-C', str(ORIGINAL), 'show', f'{COMMIT}:frontend/{row["path"]}'])
    assert path.read_bytes() == pinned and sha(path) == row['sha256'], row['path']
    frontend.append(dict(row, unchanged=True))
monaco = []
installed = SOURCE / 'frontend/node_modules/monaco-editor'
for row in prior['installedMonacoFiles']:
    assert sha(installed / row['path']) == row['sha256'], row['path']
    monaco.append(row)
extra = []
for name in ['esm/vs/base/browser/ui/hover/hover.css', 'esm/vs/editor/contrib/hover/browser/contentHover.js', 'esm/vs/editor/contrib/hover/browser/markerHoverParticipant.js']:
    extra.append({'path': name, 'sha256': sha(installed / name)})
assert json.loads((installed / 'package.json').read_text())['version'] == '0.47.0'
# The source lock is unchanged. The separately installed dependency copy changes
# only seven mirror hosts; versions and integrity strings are exactly retained.
lock = json.loads((SOURCE / 'frontend/package-lock.json').read_text())
resolved = json.loads(Path('/private/tmp/rust-rdm-footer-node-deps/package-lock.json').read_text())
assert lock.keys() == resolved.keys()
changes = 0
for key, value in lock['packages'].items():
    other = resolved['packages'][key]
    if value == other:
        continue
    assert {k: v for k, v in value.items() if k != 'resolved'} == {k: v for k, v in other.items() if k != 'resolved'}
    assert value['resolved'].replace('https://registry.npmmirror.com/', 'https://registry.npmjs.org/') == other['resolved']
    changes += 1
assert changes == 7
fixture = json.loads((ROOT / 'tests/fixtures/json-diagnostic-footer-source.json').read_text())
expected_counts = {'character': 64, 'word': 25, 'boundaries': 43, 'crossmessage': 25}
counts = {}
keys = 0
for case, count in expected_counts.items():
    raw = json.loads((CAPTURES / f'{case}-raw.json').read_text())
    actual = [r for r in raw['records'] if r['name'] not in ['initial-dom-range', 'before-local-AppKit-drag']]
    compact = fixture['sequences'][case]
    assert len(actual) == len(compact) == count
    initial = actual[0]['state']
    assert initial['selected'] == '中文🙂' and len(initial['pointerEvents']) == 2
    assert all(e['trusted'] for e in initial['pointerEvents'])
    for i, (row, target) in enumerate(zip(actual, compact)):
        state = row['state']
        assert row['name'] == target['name'] and state['selected'] == target['selected']
        assert state['value'] == fixture['modelValue'] and not state['editorHasTextFocus'] and state['entryVisible']
        assert initial['markerBox']['y'] - state['markerBox']['y'] == target['hoverScrollTop']
        if i:
            events = state['events']
            assert len(events) == i and events[-1]['trusted'] and not events[-1]['initialPrevented']
            event = events[-1]
            key = ('alt-' if event['alt'] else '') + 'shift-' + ('left' if event['key'] == 'ArrowLeft' else 'right')
            assert event['key'] in ['ArrowLeft', 'ArrowRight'] and event['shift'] and not event['meta'] and key == target['key']
            keys += 1
    counts[case] = len(actual)
save = json.loads((CAPTURES / 'save-raw.json').read_text())
rows = [r for r in save['records'] if r['name'] not in ['initial-dom-range', 'before-local-AppKit-drag']]
assert [r['state']['selected'] for r in rows] == ['No', 'No', 'No ', '']
assert all(e['trusted'] for e in rows[0]['state']['pointerEvents'])
end = rows[-1]['state']
assert all(e['trusted'] for e in end['events']) and len(end['saveCalls']) == 1
assert end['saveCalls'][0] == {'editorHasTextFocus': False, 'selected': 'No '}
assert not end['entryVisible']
request = end['requests'][0]
assert request['url'] == '/api/browser/set-hash-value' and request['method'] == 'POST' and request['status'] == 200
assert json.loads(request['body']) == fixture['save']['expectedRequest']
assert fixture['save']['expectedRequest']['value'] == '/*'
command = json.loads((CAPTURES / 'show-command-raw.json').read_text())['records']
assert len(command) == len(fixture['showCommand']) == 6
assert fixture['postShowEditorRelayout'] is False
for i, (row, compact) in enumerate(zip(command, fixture['showCommand'])):
    state = row['state']
    assert row['name'] == compact['name'] and state['value'] == fixture['showCommandModelValue'] == '{"a":}'
    assert all(event['trusted'] for event in state['events']) and len(state['events']) == i
    for key in ['hover', 'hoverBox', 'editorHasTextFocus']:
        assert compact[key] == state[key]
assert [r['hover'] for r in fixture['showCommand']] == [False, False, True, True, True, False]
assert [r['editorHasTextFocus'] for r in fixture['showCommand']] == [True, True, True, True, False, True]
files = [ROOT / 'tests/fixtures/json-diagnostic-footer-source.json', Path(__file__)] + sorted(p for p in CAPTURES.iterdir() if p.is_file())
report = {
    'checkedAt': datetime.now(timezone.utc).isoformat(), 'sourceCommit': COMMIT,
    'frontendFiles': frontend, 'installedMonacoFiles': monaco, 'additionalFooterSources': extra,
    'officialRegistryHostReplacements': changes, 'sourceStateCounts': counts, 'sourceStateTotal': sum(counts.values()),
    'trustedKeyboardEvents': keys, 'showCommandStates': len(command), 'showCommandKeyboardEvents': 5, 'postShowEditorRelayout': False, 'footerSaveStates': len(rows), 'footerSaveKeyboardEvents': len(end['events']),
    'files': [{'path': str(p.relative_to(ROOT)), 'sha256': sha(p)} for p in files],
    'scope': fixture['scope'] + ' Also checks one footer mouse/select-all/extend/save sequence against the local mock RPC. Hidden WKWebView selections are inactive gray; screenshots do not prove active native selection colors or full-image parity.',
    'fullFunctionalParityComplete': False, 'fullVisualParityComplete': False, 'passed': True,
}
(ROOT / 'outputs/json-diagnostic-footer-source-check.json').write_text(json.dumps(report, ensure_ascii=False, indent=2) + '\n')
print(f'FOOTER SOURCE PASS: {sum(counts.values())} states, {keys} trusted arrow events, 4 footer-save and 6 show/focus command states; 13 original frontend and 16 installed Monaco hashes')
