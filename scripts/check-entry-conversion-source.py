#!/usr/bin/env python3
"""Check provenance and delayed conversion observations from the original UI."""
import hashlib
import json
import subprocess
from pathlib import Path

root = Path(__file__).resolve().parents[1]
original = Path('/Users/zlsj/Projects/tiny-rdm')
temporary = Path('/private/tmp/rust-rdm-source-qa')
commit = subprocess.check_output(['git', 'rev-parse', 'HEAD'], cwd=original, text=True).strip()
assert commit == '4ddb94a3c55f3401dbc66870f0d21cf0d6203cbc'
base = json.loads((root / 'outputs/entry-picker-source-check.json').read_text())
assert base['passed'] and base['sourceCommit'] == commit
files = []
for item in base['frontendFiles']:
    path = item['path']
    data = (original / 'frontend' / path).read_bytes()
    assert data == (temporary / path).read_bytes()
    assert data == subprocess.check_output(['git', 'show', 'HEAD:frontend/' + path], cwd=original)
    files.append({'path': path, 'sha256': hashlib.sha256(data).hexdigest(), 'unchanged': True})
fixture = json.loads((root / 'tests/fixtures/entry-conversion-source.json').read_text())
assert fixture['sourceCommit'] == commit
events = {event['name']: event for event in fixture['events']}
assert len(events) == len(fixture['events']) == 11
assert len(fixture['records']) == 2
first = events['entry-conversion-pending']
assert first['field'] == 'draft-field' and first['lines'] == [''] and first['enabled']
assert first['format'] == 'Raw' and first['decode'] == 'None'
assert not first['formatDisabled'] and not first['decodeDisabled']
assert first['requests'] == [{'value': 'before', 'decode': 'None', 'format': 'JSON'}]
second = events['entry-conversion-followup']
assert second['field'] == 'old' and second['lines'] == [''] and second['enabled']
assert second['format'] == 'JSON' and len(second['requests']) == 2 and len(second['pending']) == 1
settled = events['entry-conversion-settled']
assert settled['lines'] == ['second converted', 'value'] and not settled['enabled']
assert len(settled['requests']) == 2 and not settled['pending']
assert events['newer-first-reply']['format'] == 'YAML'
assert events['older-late-reply']['format'] == 'JSON'
chain = events['race-followup-chain']
assert [request['format'] for request in chain['requests']] == ['JSON', 'YAML'] + ['YAML', 'JSON'] * 3
assert len(chain['pending']) == 2
assert events['entry-conversion-edit-while-pending']['lines'] == ['typed while waiting']
saved = events['save-while-converting']
assert saved['editorClosed'] and saved['pendingConversions'] == 1
assert saved['request']['body']['value'] == 'typed while waiting'
assert saved['request']['body']['newField'] == 'draft-field'
assert saved['request']['body']['format'] == 'Raw'
under_settings = events['conversion-finishes-under-settings']
assert under_settings['lines'] == ['under settings final'] and not under_settings['enabled']
assert len(under_settings['requests']) == 2 and not under_settings['pending']
edge = json.loads((root / 'tests/fixtures/entry-conversion-edge-source.json').read_text())
assert edge['sourceCommit'] == commit and len(edge['events']) == 6
assert len(edge['records']) == 4
boundaries = {event['name']: event for event in edge['events']}
initial = boundaries['normalized-initial-pending']
assert initial['field'] == '' and initial['placeholder'] == 'old' and initial['enabled']
assert initial['requests'] == [{'value': 'before', 'decode': 'None', 'format': 'Raw'}]
assert boundaries['normalized-initial-ready']['lines'] == ['before']
assert boundaries['empty-baseline-pending']['enabled']
for name in ('empty-baseline-followup', 'empty-baseline-ready'):
    assert boundaries[name]['lines'] == ['visible draft'] and not boundaries[name]['enabled']
assert boundaries['empty-baseline-editor-shortcut']['request']['body']['value'] == ''
assert boundaries['empty-baseline-editor-shortcut']['editorClosed']
report = {'sourceCommit': commit, 'frontendFiles': files, 'interactionChecks': 11,
          'pendingLayoutReferences': 2, 'singleSelectionRequests': 2,
          'edgeObservations': 6, 'edgeScope': edge['scope'],
          'edgeLayoutReferences': 4,
          'boundedRaceRequests': 8, 'scope': fixture['scope'],
          'verificationScope': 'Source frontend only; native runtime checks are separate.',
          'nativeConversionParityVerified': False, 'passed': True}
(root / 'outputs/entry-conversion-source-check.json').write_text(
    json.dumps(report, ensure_ascii=False, indent=2) + '\n')
print('SOURCE ENTRY CONVERSION PASS: 11 unchanged files, 11 delayed-RPC and six edge observations; Source evidence only')
