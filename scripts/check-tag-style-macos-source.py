#!/usr/bin/env python3
"""Validate original WKWebView matched-tag evidence, separately from GPUI."""
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


def sha(path):
    return hashlib.sha256(path.read_bytes()).hexdigest()


def xml(node):
    return [node.tag.rsplit('}', 1)[-1], dict(node.attrib), [xml(c) for c in node]]


f = json.loads((ROOT / 'tests/fixtures/tag-style-macos-source.json').read_text())
raw = json.loads((ROOT / f['rawReport']).read_text())
assert sha(ROOT / f['rawReport']) == f['rawSHA256']
assert f['records'] == raw['records'] and len(f['records']) == 6
assert f['sourceEngine'] == 'macOS WKWebView'
assert f['sourceCommit'] == '4ddb94a3c55f3401dbc66870f0d21cf0d6203cbc'
a = ROOT / f['archiveDirectory']
p = json.loads((ROOT / f['preparation']).read_text())
e = json.loads((ROOT / f['execution']).read_text())
assert e['passed'] and e['exitCode'] == 0 and e['allInputHashesUnchanged']
assert sha(a / 'stdout-attempt1.json') == e['stdoutSHA256']
assert sha(a / 'stderr-attempt1.log') == e['stderrSHA256']
assert (a / 'stderr-attempt1.log').read_bytes() == b''
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
    assert row['sourceEventsAreTrusted'] is False
    assert row['match'] == 'hash' and row['inputFocused'] is True
    assert row['filter'] == ('list' if config['draft'] else '')
    assert row['searchDisabled'] is (not config['draft'])
    m = {s['id']: s for s in row['styles']}
    dark = config['theme'] == 'dark'
    assert m['tag']['borderRadius'] == m['border']['borderRadius'] == '4px'
    assert m['tag']['h'] == 22 and m['content']['fontSize'] == '12px'
    assert m['tag']['background'] == ('rgba(0, 0, 0, 0)' if dark else 'rgb(250, 250, 252)')
    assert m['border']['border'] == ('1px solid rgb(81, 81, 81)' if dark else '1px solid rgb(224, 224, 230)')
    assert m['close']['w'] == m['close']['h'] == 12
    assert m['closeIcon']['w'] == m['closeIcon']['h'] == 16
    assert m['closeIcon']['x'] == m['close']['x'] - 2
    assert m['closeIcon']['y'] == m['close']['y'] - 2
    assert m['close']['color'] == ('rgba(255, 255, 255, 0.52)' if dark else 'rgb(102, 102, 102)')
    assert m['search']['opacity'] == ('1' if config['draft'] else '0.38' if dark else '0.5')
    assert xml(ET.fromstring(m['closeIcon']['svg'])) == xml(ET.fromstring((ROOT / 'assets/icons/search-match-close.svg').read_text()))
    assert struct.unpack('>II', (a / (row['name'] + '.png')).read_bytes()[16:24]) == (2048, 1536)
    assert json.loads((a / (row['name'] + '.json')).read_text()) == row

report = {'passed': True, 'sourceStates': 6, 'pinnedFrontendFiles': len(f['pinnedFrontendSources']),
          'dependencyFiles': len(f['installedDependencySources']), 'generalPointSizes': [14, 20],
          'tagRadius': 4, 'tagHeight': 22, 'tagTextPointSize': 12,
          'closeHitSize': 12, 'closeSVGSize': 16, 'sourceCloseVectorMatchesAsset': True,
          'disabledOpacity': {'light': 0.5, 'dark': 0.38}, 'sourceEventsAreTrusted': False,
          'scope': 'Original bytes, DOM/state and fixed model integrity only. No GPUI pixels, whole-image, actual Redis or native acceptance.'}
(ROOT / 'outputs/tag-style-macos-source-check.json').write_text(json.dumps(report, indent=2) + '\n')
print('MATCH TAG ORIGINAL SOURCE PASS:6 states, radius4, hit12/SVG16, disabled opacity0.5/0.38')
