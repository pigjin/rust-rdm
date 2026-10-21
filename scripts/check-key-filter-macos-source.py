#!/usr/bin/env python3
"""Original macOS search-control evidence; actual GPUI/native checks separate."""
import hashlib
import json
import struct
import subprocess
import xml.etree.ElementTree as ET
from pathlib import Path

ROOT = Path(__file__).resolve().parents[1]
ORIGINAL = Path('/Users/zlsj/Projects/tiny-rdm')
SOURCE = Path('/private/tmp/rust-rdm-footer-source-qa')
DEPS = Path('/private/tmp/rust-rdm-footer-node-deps/node_modules')

def sha(p):
    return hashlib.sha256(p.read_bytes()).hexdigest()

f = json.loads((ROOT / 'tests/fixtures/key-filter-macos-source.json').read_text())
raw = json.loads((ROOT / f['rawReport']).read_text())
assert sha(ROOT / f['rawReport']) == f['rawSHA256']
assert f['records'] == raw['records'] and len(f['records']) == 22
assert f['sourceEngine'] == 'macOS WKWebView'
assert f['sourceCommit'] == '4ddb94a3c55f3401dbc66870f0d21cf0d6203cbc'
a = ROOT / f['archiveDirectory']
p = json.loads((ROOT / f['preparation']).read_text())
e = json.loads((ROOT / f['execution']).read_text())
assert e['passed'] and e['exitCode'] == 0 and e['allInputHashesUnchanged']
assert sha(a / 'stdout-attempt3.json') == e['stdoutSHA256'] and sha(a / 'stderr-attempt3.log') == e['stderrSHA256']
assert (a / 'stderr-attempt3.log').read_bytes() == b''
for row in f['files']:
    assert sha(ROOT / row['path']) == row['sha256'], row['path']
for row in p['inputs']:
    assert sha(a / Path(row['path']).name) == row['sha256']
for row in f['pinnedFrontendSources']:
    data = subprocess.check_output(['git', '-C', str(ORIGINAL), 'show', f['sourceCommit'] + ':' + row['path']])
    assert data == (ORIGINAL / row['path']).read_bytes() == (SOURCE / row['path']).read_bytes() == (a / row['path']).read_bytes()
    assert hashlib.sha256(data).hexdigest() == row['sha256']
for row in f['installedDependencySources']:
    assert sha(DEPS / row['path']) == sha(a / 'dependencies' / row['path']) == row['sha256']
for config, row in zip(p['configurations'], f['records']):
    assert row['configuration'] == config and row['name'] == config['name']
    assert row['inputEventsAreTrusted'] is False and not row['errorNotifications']
    assert row['appFontSize'] == str(config['size']) + 'px'
    metrics = {m['id']: m for m in row['metrics']}
    assert metrics['input']['fontSize'] == '14px' and metrics['outer']['h'] == 28
    assert metrics['clear']['w'] == metrics['clear']['h'] == 16
    assert metrics['exact']['w'] == 24 and metrics['exact']['h'] == 22
    assert metrics['header']['h'] == 38.171875
    if config['family']:
        assert config['family'] in metrics['input']['fontFamily']
    assert struct.unpack('>II', (a / (row['name'] + '.png')).read_bytes()[16:24]) == (2048, 1536)
    assert json.loads((a / (row['name'] + '.json')).read_text()) == row

vector = next(m['svg'] for row in f['records'] for m in row['metrics'] if m['id'] == 'clearIcon')
def xml(node):
    return [node.tag.rsplit('}', 1)[-1], dict(node.attrib), [xml(c) for c in node]]
assert xml(ET.fromstring(vector)) == xml(ET.fromstring((ROOT / 'assets/icons/input-clear.svg').read_text()))
for row in f['records']:
    m={x['id']:x for x in row['metrics']}
    assert m['exactIcon']['h']==14 and m['exactIcon']['y']-m['exact']['y']==3

report = {'passed': True, 'sourceStates': 22, 'pinnedFrontendFiles': len(f['pinnedFrontendSources']),
          'dependencyFiles': len(f['installedDependencySources']), 'inputPointSize': 14,
          'generalPointSizes': [14, 20], 'clearSlotAndIconSize': 16, 'clearVectorMatchesDOM': True,
          'sourceEventsAreTrusted': False,
          'scope': 'Original bytes, DOM/state and fixed model integrity only. No GPUI pixels, whole-image, actual Redis or native acceptance.'}
(ROOT / 'outputs/key-filter-macos-source-check.json').write_text(json.dumps(report, indent=2) + '\n')
print('KEY FILTER ORIGINAL SOURCE PASS:22 states, fixed14px, reserved16px clear slot and exact source SVG')
