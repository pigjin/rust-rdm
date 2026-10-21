#!/usr/bin/env python3
"""Verify the twelve original matched entry-input width samples."""
import hashlib
import json
import struct
import subprocess
from pathlib import Path

ROOT = Path(__file__).resolve().parents[1]
fixture = json.loads((ROOT / 'tests/fixtures/entry-match-macos-source.json').read_text())
prep = json.loads((ROOT / fixture['preparation']).read_text())
execution = json.loads((ROOT / fixture['execution']).read_text())
archive = ROOT / fixture['archiveDirectory']
sha = lambda path: hashlib.sha256(path.read_bytes()).hexdigest()
assert fixture['sourceCommit'] == '4ddb94a3c55f3401dbc66870f0d21cf0d6203cbc'
assert execution['passed'] and execution['exitCode'] == 0 and execution['allInputHashesUnchanged']
assert sha(ROOT / fixture['rawReport']) == fixture['rawSHA256'] == execution['stdoutSHA256'] == sha(archive / 'stdout-attempt2.json')
assert sha(archive / 'stderr-attempt2.log') == execution['stderrSHA256']
assert not (archive / 'stderr-attempt2.log').read_bytes()
assert fixture['records'] == json.loads((ROOT / fixture['rawReport']).read_text())['records']
assert len(fixture['records']) == len(prep['configurations']) == 12
for row in fixture['files']:
    assert sha(ROOT / row['path']) == row['sha256'], row['path']
for row in prep['inputs']:
    assert sha(archive / Path(row['path']).name) == row['sha256']
for row in fixture['pinnedFrontendSources']:
    data = subprocess.check_output(['git', '-C', '/Users/zlsj/Projects/tiny-rdm', 'show', fixture['sourceCommit'] + ':' + row['path']])
    assert data == (archive / row['path']).read_bytes() == (Path('/Users/zlsj/Projects/tiny-rdm') / row['path']).read_bytes() == (Path('/private/tmp/rust-rdm-footer-source-qa') / row['path']).read_bytes()
    assert hashlib.sha256(data).hexdigest() == row['sha256']
for row in fixture['installedDependencySources']:
    assert sha(archive / 'dependencies' / row['path']) == sha(Path('/private/tmp/rust-rdm-footer-node-deps/node_modules') / row['path']) == row['sha256']
checks = []
for c, row in zip(prep['configurations'], fixture['records']):
    assert row['configuration'] == c and row['hoverTooltipRetains'] and row['leavingHides']
    assert not row['sourceEventsAreTrusted'] and not row['inputFocused'] and row['inputValue'] == ''
    assert row['fullMatch'] == c['label'] and row['tooltipText'] == '内容已匹配为 *' + c['label'] + '*'
    styles = {v['id']: v for v in row['styles']}
    group, outer, search, tag = [styles[k] for k in ['group', 'outer', 'search', 'tag']]
    assert group['x'] == outer['x'] == 356 and group['y'] == outer['y'] == search['y'] == 121
    assert group['h'] == outer['h'] == search['h'] == 32 and tag['h'] == 22
    assert abs(group['w'] - (outer['w'] + search['w'] - 1)) < 0.001
    assert search['x'] == outer['x'] + outer['w'] - 1
    assert group['w'] <= (c['windowWidth'] - c['asideWidth'] - 62) / 2
    assert tag['x'] == 362 and tag['y'] == 126 and tag['w'] <= (outer['w'] - 12) / 2
    assert styles['tooltip']['fontSize'] == '14px' and styles['tooltip']['h'] == 37
    assert struct.unpack('>II', (archive / (row['name'] + '.png')).read_bytes()[16:24]) == (c['windowWidth'] * 2, 1536)
    assert json.loads((archive / (row['name'] + '.json')).read_text()) == row
    checks.append({'scene': row['name'], 'groupWidth': group['w'], 'inputWidth': outer['w'], 'tagWidth': tag['w']})
report = {'passed': True, 'sourceStates': 12, 'checks': checks, 'pinnedFrontendFiles': len(fixture['pinnedFrontendSources']), 'dependencyFiles': len(fixture['installedDependencySources']), 'scope': 'Original twelve actual WKWebView component widths and tooltip rectangles/PNG hashes, 1024/1340/System/Courier only. Full retry passed; prior seven-capture timeout retained separately. GPUI, native and strict paint acceptance separate.'}
(ROOT / 'outputs/entry-match-macos-source-check.json').write_text(json.dumps(report, ensure_ascii=False, indent=2) + '\n')
print('ORIGINAL ENTRY MATCH SOURCE PASS:12 states, input/group/tag widths, pinned sources and dependencies')
