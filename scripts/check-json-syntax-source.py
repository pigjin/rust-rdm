#!/usr/bin/env python3
"""Verify the captured original JSON lexical colors without accepting GPUI output."""
import hashlib
import json
from pathlib import Path
import subprocess

ROOT = Path(__file__).resolve().parents[1]
report = json.loads((ROOT / 'outputs/json-syntax-source-check.json').read_text())
fixture_path = ROOT / 'tests/fixtures/json-syntax-source.json'
fixture = json.loads(fixture_path.read_text())
assert hashlib.sha256(fixture_path.read_bytes()).hexdigest() == report['fixtureSHA256']
assert fixture['sourceCommit'] == report['sourceCommit'] == '4ddb94a3c55f3401dbc66870f0d21cf0d6203cbc'
assert len(fixture['samples']) == report['samples'] == 36
for row in report['files']:
    assert hashlib.sha256((ROOT / row['path']).read_bytes()).hexdigest() == row['sha256'], row['path']
original = Path('/Users/zlsj/Projects/tiny-rdm')
for row in report['frontendFiles']:
    expected = subprocess.check_output(['git', '-C', str(original), 'show', report['sourceCommit'] + ':frontend/' + row['path']])
    assert expected == (original / 'frontend' / row['path']).read_bytes()
    assert hashlib.sha256(expected).hexdigest() == row['sha256']
monaco = Path('/private/tmp/rust-rdm-footer-node-deps/node_modules/monaco-editor')
for row in report['installedMonacoFiles']:
    assert hashlib.sha256((monaco / row['path']).read_bytes()).hexdigest() == row['sha256']
for sample in fixture['samples']:
    lines = sample['text'].replace('\r\n', '\n').split('\n')
    assert len(lines) == len(sample['lines']) == len(sample['tokens'])
    assert all(''.join(span['text'] for span in row['spans']) == text for text, row in zip(lines, sample['lines']))
assert sum(sample['dark'] for sample in fixture['samples']) == 18
result = {
    'passed': True, 'sourceCommit': report['sourceCommit'], 'samples': 36,
    'frontendFilesChecked': len(report['frontendFiles']), 'installedMonacoFilesChecked': len(report['installedMonacoFiles']),
    'archivedCaptureFilesChecked': len(report['files']),
    'scope': 'Captured source text, token/color records and archive hashes only. No GPUI pixels, native interaction or full parity acceptance.',
}
(ROOT / 'outputs/json-syntax-source-verification.json').write_text(json.dumps(result, indent=2) + '\n')
print('JSON SYNTAX SOURCE PASS: 36 original color/token samples and pinned capture hashes')
