#!/usr/bin/env python3
"""Verify original Mac collection evidence; GPUI/native acceptance is separate."""
import hashlib
import json
import struct
import subprocess
import xml.etree.ElementTree as ET
from pathlib import Path

ROOT = Path(__file__).resolve().parents[1]
SOURCE = Path('/private/tmp/rust-rdm-footer-source-qa')
ORIGINAL = Path('/Users/zlsj/Projects/tiny-rdm')
DEPS = Path('/private/tmp/rust-rdm-footer-node-deps/node_modules')
COMMIT = '4ddb94a3c55f3401dbc66870f0d21cf0d6203cbc'


def read(name):
    return json.loads((ROOT / name).read_text())


def sha(path):
    return hashlib.sha256(path.read_bytes()).hexdigest()


def has(row, name):
    return name in row['class'].split()


frontends, dependencies = set(), set()
for filename, count in [('entries-macos-source.json', 24), ('entries-overlays-macos-source.json', 7)]:
    fixture = read('tests/fixtures/' + filename)
    assert fixture['sourceCommit'] == COMMIT and fixture['sourceEngine'] == 'macOS WKWebView'
    assert sha(ROOT / fixture['rawReport']) == fixture['rawSHA256']
    raw = read(fixture['rawReport'])
    assert len(fixture['records']) == len(raw['records']) == count
    assert fixture['records'] == raw['records']
    archive = ROOT / fixture['archiveDirectory']
    preparation = read(fixture['preparation'])
    execution = read(fixture['execution'])
    assert execution['exitCode'] == 0 and execution['allInputHashesUnchanged']
    assert sha(archive / 'stdout.json') == execution['stdoutSHA256']
    assert sha(archive / 'stderr.log') == execution['stderrSHA256']
    assert (archive / 'stderr.log').read_bytes() == b''
    for row in fixture['files']:
        assert sha(ROOT / row['path']) == row['sha256'], row['path']
    for row in preparation['inputs']:
        assert sha(archive / Path(row['path']).name) == row['sha256']
    for row in fixture['pinnedFrontendSources']:
        pinned = subprocess.check_output(['git', '-C', str(ORIGINAL), 'show', COMMIT + ':' + row['path']])
        assert pinned == (ORIGINAL / row['path']).read_bytes() == (SOURCE / row['path']).read_bytes() == (archive / row['path']).read_bytes()
        assert hashlib.sha256(pinned).hexdigest() == row['sha256']
        frontends.add(row['path'])
    for row in fixture['installedDependencySources']:
        assert sha(DEPS / row['path']) == row['sha256']
        dependencies.add(row['path'])
    for row in fixture['records']:
        assert row['inputEventsAreTrusted'] is False and not row['errorNotifications']
        assert row['appFontSize'] == str(row['generalFontSize']) + 'px'
        data = (archive / (row['name'] + '.png')).read_bytes()
        assert data[:8] == b'\x89PNG\r\n\x1a\n'
        assert struct.unpack('>II', data[16:24]) == (row['width'] * 2, 1536)
        assert json.loads((archive / (row['name'] + '.json')).read_text()) == row

fixture = read('tests/fixtures/entries-macos-source.json')
old = read('tests/fixtures/entries-source.json')
assert len(old['records']) == 14
old_hash = next(row['sha256'] for row in read('outputs/connection-tabs-footer-build-inputs.json')['inputs'] if row['path'] == 'tests/fixtures/entries-source.json')
assert sha(ROOT / 'tests/fixtures/entries-source.json') == old_hash
for config, record in zip(fixture['configurations'], fixture['records']):
    assert record['name'] == config['name'] and record['rows'] == config['rows']
    assert record['kind'] == config['kind'] and record['width'] == config['width']
    assert record['generalFontSize'] == config['size'] and record['theme'] == config['theme']
    headers = [m for m in record['metrics'] if has(m, 'n-data-table-th')]
    cells = [m for m in record['metrics'] if has(m, 'n-data-table-td')]
    assert all(m['fontSize'] == '14px' and m['fontWeight'] == '500' and m['h'] == 34 for m in headers)
    assert all(m['fontSize'] == '14px' and m['fontWeight'] == '400' and m['h'] == (61 if record['kind'] == 'STREAM' else 33) for m in cells)
    if config.get('family'):
        assert all('Menlo' in m['fontFamily'] for m in headers + cells)
    if config.get('resize'):
        base = next(r for r in fixture['records'] if r['name'] == 'entries-hash')
        original = next(m for m in base['metrics'] if has(m, 'n-data-table-th') and m['text'] == '字段')
        resized = next(m for m in headers if m['text'] == '字段')
        assert resized['w'] - original['w'] == 50
    if config.get('sort'):
        assert any(has(m, 'n-data-table-sorter--desc') for m in record['metrics'])


def canonical(node, root=False):
    attrs = dict(node.attrib)
    if root:
        attrs.pop('fill', None)
        attrs.pop('version', None)
    return [node.tag.rsplit('}', 1)[-1], attrs, [canonical(child) for child in node]]


svg = next(m['svg'] for m in fixture['records'][0]['metrics'] if m['tag'].lower() == 'svg' and 165 < m['y'] < 190)
down = ET.fromstring((ROOT / 'assets/icons/entry-sort-down.svg').read_text())
assert canonical(down, True) == canonical(ET.fromstring(svg), True)
up = ET.fromstring((ROOT / 'assets/icons/entry-sort-up.svg').read_text())
assert up[0].attrib == {'transform': 'rotate(180 14 14)'}
assert canonical(up[0][0]) == canonical(down[0])

controls = []
for name in ['input-metrics-macos-source.json', 'input-metrics-macos-extra-source.json']:
    f = read('tests/fixtures/' + name)
    assert f['sourceCommit'] == COMMIT and sha(ROOT / f['rawReport']) == f['rawSHA256']
    raw = read(f['rawReport'])
    samples = [{key: row[key] for key in ('family', 'point', 'count', 'width', 'zeroWidth')} for row in raw['source']['source']]
    assert len(samples) == 60 and samples == f['samples']
    assert len(f['nativeTablePredictions']) == 60
    for row in f['files']:
        assert sha(ROOT / row['path']) == row['sha256']
    assert all(abs(row['nativePrediction'] - row['width']) < 0.02 for row in f['nativeTablePredictions'])
    controls.extend(samples)
assert len(controls) == 120 and len({row['family'] for row in controls}) == 8
assert len(read('tests/fixtures/input-metrics-macos-source.json')['negativeNativeEstimates']) == 12
report = {'passed': True, 'sourceCommit': COMMIT, 'collectionStates': 24, 'overlayStates': 7,
          'inputControlSamples': 120, 'inputFamilies': 8, 'nativeNegativeEstimatesRetained': 12,
          'legacyChromeStatesRetained': 14, 'pinnedFrontendFiles': len(frontends), 'dependencyFiles': len(dependencies),
          'scope': 'Original source/probe integrity, explicit model inputs and public native table controls only. Programmatic source events are untrusted. GPUI layout/color/glyph and actual-native/full parity acceptance are separate.'}
(ROOT / 'outputs/entries-macos-source-check.json').write_text(json.dumps(report, indent=2) + '\n')
print('MACOS COLLECTION SOURCE VERIFIED:24 table/seven overlay states,120 input controls,eight families; legacy14 and12 native negatives retained')
