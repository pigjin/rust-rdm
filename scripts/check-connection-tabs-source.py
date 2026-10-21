#!/usr/bin/env python3
"""Verify original tab observations and source SVGs; product acceptance is separate."""
import hashlib
import json
from pathlib import Path
import subprocess
import xml.etree.ElementTree as ET

ROOT = Path(__file__).resolve().parents[1]
ORIGINAL = Path('/Users/zlsj/Projects/tiny-rdm')
report = json.loads((ROOT / 'outputs/connection-tabs-source-check.json').read_text())
path = ROOT / 'tests/fixtures/connection-tabs-source.json'
fixture = json.loads(path.read_text())
assert hashlib.sha256(path.read_bytes()).hexdigest() == report['fixtureSHA256']
assert fixture['sourceCommit'] == report['sourceCommit'] == '4ddb94a3c55f3401dbc66870f0d21cf0d6203cbc'
for row in report['files']:
    assert hashlib.sha256((ROOT / row['path']).read_bytes()).hexdigest() == row['sha256']
for row in fixture['pinnedFrontendSources']:
    pinned = subprocess.check_output(['git', '-C', str(ORIGINAL), 'show', report['sourceCommit'] + ':' + row['path']])
    assert pinned == (ORIGINAL / row['path']).read_bytes()
    assert hashlib.sha256(pinned).hexdigest() == row['sha256']
installed = Path('/private/tmp/rust-rdm-footer-node-deps/node_modules')
for row in fixture['installedDependencySources']:
    assert hashlib.sha256((installed / row['path']).read_bytes()).hexdigest() == row['sha256']
server_source = 'frontend/src/components/icons/Server.vue'
assert subprocess.check_output(['git', '-C', str(ORIGINAL), 'show', report['sourceCommit'] + ':' + server_source]) == (ORIGINAL / server_source).read_bytes()
states = fixture['programmaticBehaviorStates']
assert len(states) == 9 and len(fixture['layoutStates']) == 8
assert len({row['editorModel'] for row in states}) == 1
assert all(row['originalEditorConnected'] and row['editorValue'] == 'retained across tabs' and not row['errorNotifications'] for row in states)
assert not states[2]['originalEntryVisible'] and states[3]['originalEntryVisible']
assert not states[4]['warnings'] and len(states[5]['warnings']) == 1 and not states[6]['warnings']


def canonical(element):
    attributes = {key: value for key, value in element.attrib.items()
                  if not key.startswith('data-') and key not in ('style', 'class', 'aria-hidden')}
    return [element.tag.rsplit('}', 1)[-1], attributes, [canonical(child) for child in element]]


svgs = [node['svg'] for node in states[0]['tabs'][0]['children'] if node['tag'] == 'svg']
assert len(svgs) == 2
assets = []
for name, markup in zip(['tab-server.svg', 'notice-close.svg'], svgs):
    asset = ROOT / 'assets/icons' / name
    assert canonical(ET.fromstring(asset.read_text())) == canonical(ET.fromstring(markup)), name
    assets.append({'name': name, 'sha256': hashlib.sha256(asset.read_bytes()).hexdigest()})
result = {
    'passed': True, 'programmaticBehaviorStates': 9, 'stableSourceLayoutStates': 8,
    'pinnedFrontendFiles': len(fixture['pinnedFrontendSources']) + 1,
    'dependencyFiles': len(fixture['installedDependencySources']), 'svgStructures': assets,
    'scope': 'Original source/archive hashes, controlled model retention, modifiers and graphics attributes only. Key events are untrusted; no native, product-raster or full-parity acceptance.',
}
(ROOT / 'outputs/connection-tabs-source-verification.json').write_text(json.dumps(result, indent=2) + '\n')
print('CONNECTION TABS SOURCE VERIFIED: nine behavior/eight layout states and two exact SVG structures; product acceptance separate')
