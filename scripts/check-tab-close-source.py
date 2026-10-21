#!/usr/bin/env python3
"""Verify pinned source observations without accepting product or native parity."""
import hashlib
import json
from pathlib import Path
import subprocess

ROOT = Path(__file__).resolve().parents[1]
ORIGINAL = Path('/Users/zlsj/Projects/tiny-rdm')
report = json.loads((ROOT / 'outputs/tab-close-source-check.json').read_text())
fixture_path = ROOT / 'tests/fixtures/tab-close-source.json'
fixture = json.loads(fixture_path.read_text())
assert hashlib.sha256(fixture_path.read_bytes()).hexdigest() == report['fixtureSHA256']
assert fixture['sourceCommit'] == report['sourceCommit'] == '4ddb94a3c55f3401dbc66870f0d21cf0d6203cbc'
assert len(fixture['states']) == 34 and len(fixture['layouts']) == 2
for row in report['files']:
    assert hashlib.sha256((ROOT / row['path']).read_bytes()).hexdigest() == row['sha256'], row['path']
for row in report['frontendAndBackendSources']:
    pinned = subprocess.check_output(['git', '-C', str(ORIGINAL), 'show', report['sourceCommit'] + ':' + row['path']])
    assert pinned == (ORIGINAL / row['path']).read_bytes()
    assert hashlib.sha256(pinned).hexdigest() == row['sha256']
dependency = json.loads((ROOT / 'outputs/tab-close-dependency-check.json').read_text())
lock_bytes = (ORIGINAL / 'frontend/package-lock.json').read_bytes()
assert hashlib.sha256(lock_bytes).hexdigest() == dependency['pinnedPackageLockSHA256']
lock = json.loads(lock_bytes)
installed = Path(dependency['installedSourceRoot'])
for name, version in dependency['installedVersionsMatchOriginalLock'].items():
    assert lock['packages']['node_modules/' + name]['version'] == version
    assert json.loads((installed / name / 'package.json').read_text())['version'] == version
for row in dependency['files']:
    assert hashlib.sha256((installed / row['path']).read_bytes()).hexdigest() == row['sha256']
for theme in ['light', 'dark']:
    states = [row for row in fixture['states'] if row['theme'] == theme]
    assert len(states) == 17
    for name in ['close-button-warning', 'close-button-cancel', 'confirmed-backend-failure', 'confirmed-request-pending']:
        row = next(row for row in states if row['name'] == name)
        assert row['names'] == ['localhost'] and row['editorValue'] == 'draft retained' and row['entryVisible']
    assert not next(row for row in states if row['name'] == 'close-button-cancel')['closeCalls']
    final = next(row for row in states if row['name'] == 'confirmed-request-complete')
    assert final['names'] == [] and final['activeIndex'] == -1 and final['nav'] == 'server'
    layout = next(row for row in fixture['layouts'] if row['theme'] == theme)
    assert layout['parts']['card']['width'] == 446 and layout['parts']['card']['height'] == 137
    assert layout['parts']['card']['opacity'] == '1' and layout['parts']['card']['transform'] == 'none'
result = {
    'passed': True, 'controlledSourceStates': 34, 'stableWarningLayouts': 2,
    'pinnedSourceFiles': len(report['frontendAndBackendSources']),
    'installedDependencySourceFiles': len(dependency['files']),
    'archiveFiles': len(report['files']), 'animationTimingAccepted': False,
    'scope': 'Pinned original sources, recorded programmatic states, stable layout data and archive hashes only. No GPUI pixels or native acceptance.',
}
(ROOT / 'outputs/tab-close-source-verification.json').write_text(json.dumps(result, indent=2) + '\n')
print('TAB CLOSE SOURCE VERIFIED: 34 controlled states and two stable warning layouts; product acceptance separate')
