#!/usr/bin/env python3
"""Verify original matched-tooltip hover/layout evidence and pinned sources."""
import hashlib
import json
import struct
import subprocess
from pathlib import Path

ROOT = Path(__file__).resolve().parents[1]
fixture = json.loads((ROOT / 'tests/fixtures/match-tooltip-macos-source.json').read_text())
prep = json.loads((ROOT / fixture['preparation']).read_text())
execution = json.loads((ROOT / fixture['execution']).read_text())
archive = ROOT / fixture['archiveDirectory']
sha = lambda path: hashlib.sha256(path.read_bytes()).hexdigest()
assert execution['passed'] and execution['exitCode'] == 0 and execution['allInputHashesUnchanged']
assert sha(ROOT / fixture['rawReport']) == fixture['rawSHA256'] == execution['stdoutSHA256'] == sha(archive / 'stdout-attempt3.json')
assert sha(archive / 'stderr-attempt3.log') == execution['stderrSHA256'] and not (archive / 'stderr-attempt3.log').read_bytes()
assert fixture['records'] == json.loads((ROOT / fixture['rawReport']).read_text())['records']
assert len(fixture['records']) == len(prep['configurations']) == 8
for row in fixture['files']: assert sha(ROOT / row['path']) == row['sha256'], row['path']
for row in prep['inputs']: assert sha(archive / Path(row['path']).name) == row['sha256']
for row in fixture['pinnedFrontendSources']:
    data = subprocess.check_output(['git', '-C', '/Users/zlsj/Projects/tiny-rdm', 'show', fixture['sourceCommit'] + ':' + row['path']])
    assert data == (archive / row['path']).read_bytes() == (Path('/Users/zlsj/Projects/tiny-rdm') / row['path']).read_bytes() == (Path('/private/tmp/rust-rdm-footer-source-qa') / row['path']).read_bytes()
    assert hashlib.sha256(data).hexdigest() == row['sha256']
for row in fixture['installedDependencySources']: assert sha(archive / 'dependencies' / row['path']) == sha(Path('/private/tmp/rust-rdm-footer-node-deps/node_modules') / row['path']) == row['sha256']
checks = []
for c, row in zip(prep['configurations'], fixture['records']):
    assert row['configuration'] == c and row['hoverTooltipRetains'] and row['leavingHides']
    assert not row['sourceEventsAreTrusted'] and not row['inputFocused'] and row['inputValue'] == ''
    assert row['fullMatch'] == c['label']
    pattern = c['label'] if c['host'] == 'key' else '*' + c['label'] + '*'
    assert row['tooltipText'] == '内容已匹配为 ' + pattern
    styles = {v['id']: v for v in row['styles']}; tip = styles['tooltip']
    assert tip['h'] == 37 and tip['padding'] == '8px 14px' and tip['fontSize'] == '14px' and tip['lineHeight'] == '21px'
    assert 'Courier' not in tip['fontFamily']
    assert row['placement'] == ('bottom-start' if c['host'] == 'key' else 'bottom')
    assert struct.unpack('>II', (archive / (row['name'] + '.png')).read_bytes()[16:24]) == (2048, 1536)
    assert json.loads((archive / (row['name'] + '.json')).read_text()) == row
    checks.append({'scene': row['name'], 'bounds': [tip[k] for k in ['x', 'y', 'w', 'h']], 'placement': row['placement']})
report = {'passed': True, 'sourceStates': 8, 'checks': checks, 'pinnedFrontendFiles': len(fixture['pinnedFrontendSources']), 'dependencyFiles': len(fixture['installedDependencySources']), 'scope': 'Original eight actual component DOM hover, stable popup/arrow rectangles and PNG hashes only; prior wrong-trigger setup failures retained. Actual GPUI, native, shadows/transitions/full-image acceptance separate.'}
(ROOT / 'outputs/match-tooltip-macos-source-check.json').write_text(json.dumps(report, ensure_ascii=False, indent=2) + '\n')
print('ORIGINAL MATCH TOOLTIP SOURCE PASS:8 states, font/placement/pattern/hover, pinned sources and dependencies')
