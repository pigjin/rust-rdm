#!/usr/bin/env python3
"""Verify bounded original macOS footer evidence; product acceptance is separate."""
import hashlib
import json
import subprocess
from pathlib import Path

ROOT = Path(__file__).resolve().parents[1]
ORIGINAL = Path('/Users/zlsj/Projects/tiny-rdm')
SOURCE = Path('/private/tmp/rust-rdm-footer-source-qa')
DEPENDENCIES = Path('/private/tmp/rust-rdm-footer-node-deps/node_modules')
COMMIT = '4ddb94a3c55f3401dbc66870f0d21cf0d6203cbc'


def read(name):
    return json.loads((ROOT / name).read_text())


def digest(path):
    return hashlib.sha256(path.read_bytes()).hexdigest()


font = read('tests/fixtures/footer-font-macos-source.json')
layout = read('tests/fixtures/value-footer-macos-source.json')
assert font['sourceCommit'] == layout['sourceCommit'] == COMMIT
assert font['sourceEngine'] == layout['sourceEngine'] == 'macOS WKWebView'
for fixture in (font, layout):
    assert digest(ROOT / fixture['rawReport']) == fixture['rawSHA256']
    for row in fixture['files']:
        assert digest(ROOT / row['path']) == row['sha256'], row['path']

raw = read(font['rawReport'])
assert font['fontFamily'] == raw['source']['family']
assert font['samples'] == [
    {key: row[key] for key in ('size', 'text', 'width')}
    for row in raw['source']['samples']
]
assert len(font['samples']) == 40
assert {row['size'] for row in font['samples']} == {12, 14, 16, 20}
chrome = read('tests/fixtures/footer-font-source.json')
differences = []
for sample in font['samples']:
    old = next(row for row in chrome['samples']
               if (row['size'], row['text']) == (sample['size'], sample['text']))
    if abs(old['width'] - sample['width']) >= 0.1:
        differences.append(dict(size=sample['size'], text=sample['text'],
                                WKWidth=sample['width'], ChromeWidth=old['width']))
assert len(differences) == 5 and all(row['size'] == 20 for row in differences)
assert differences == read('outputs/footer-font-webkit-vs-chrome.json')['differences']
assert digest(ROOT / 'tests/fixtures/footer-font-source.json') == next(
    row['sha256'] for row in read('outputs/connection-tabs-build-inputs.json')['inputs']
    if row['path'] == 'tests/fixtures/footer-font-source.json')
assert json.loads((ROOT / 'outputs/source-probes/footer-font-webkit/footer40.stdout.log').read_text()) == raw

preparation = read(layout['preparation'])
execution = read(layout['execution'])
assert execution['exitCode'] == 0
assert all(execution[key] for key in ('inputHashesUnchanged', 'sourceHashesUnchanged', 'dependencyHashesUnchanged'))
archive = ROOT / layout['archiveDirectory']
for row in preparation['inputs']:
    assert digest(archive / Path(row['path']).name) == row['sha256']
assert digest(archive / 'stdout.json') == execution['stdoutSHA256']
assert digest(archive / 'stderr.log') == execution['stderrSHA256']
assert (archive / 'stderr.log').read_bytes() == b''
layout_raw = read(layout['rawReport'])
assert layout_raw == json.loads((archive / 'stdout.json').read_text())
for original, compact in zip(layout_raw['records'], layout['records']):
    normalized = dict(original, metrics=[dict(row, text=row['text'].strip()) for row in original['metrics']])
    assert normalized == compact
assert len(layout_raw['records']) == len(layout['records']) == 6
expected = ['footer-hash', 'footer-decode-long', 'footer-decode-long-menu',
            'footer-hash20', 'footer-decode-long20', 'footer-decode-long-menu20']
assert [row['name'] for row in layout['records']] == expected
for row in layout['records']:
    assert row['declaredBuiltInDecoders'] == ['Base64', 'GZip', 'Deflate', 'ZStd', 'LZ4', 'Brotli', 'Msgpack', 'PHP', 'Pickle']
    assert row['generalFontSize'] == (20 if row['name'].endswith('20') else 14)
    assert not row['errorNotifications'] and row['menuCount'] == int('-menu' in row['name'])
    footer = next(m for m in row['metrics'] if 'value-footer' in m['class'].split())
    assert footer['fontSize'] == str(row['generalFontSize']) + 'px'
    buttons = [m for m in row['metrics'] if 'n-button' in m['class'].split()]
    assert len(buttons) == 2 and all(m['fontSize'] == '14px' and m['h'] == 32 for m in buttons)
    assert buttons[0]['text'] == 'Raw'
    assert buttons[1]['text'] == ('自定义解码器ABCDEFGHIJKLMN' if 'decode-long' in row['name'] else 'None')
    options = [m['text'] for m in row['metrics'] if 'n-dropdown-option-body' in m['class'].split()]
    assert options == (['None', 'GZip', 'Deflate', 'ZStd', 'LZ4', 'Brotli', 'Msgpack', 'PHP', 'Pickle', 'Base64', '自定义解码器ABCDEFGHIJKLMN', '添加自定义解码'] if row['menuCount'] else [])
    for metric in row['metrics']:
        assert all(isinstance(metric[k], (int, float)) for k in ('x', 'y', 'w', 'h'))

assert layout['pinnedFrontendSources'] == preparation['pinnedFrontendSources']
assert layout['installedDependencySources'] == preparation['installedDependencySources']
for row in layout['pinnedFrontendSources']:
    pinned = subprocess.check_output(['git', '-C', str(ORIGINAL), 'show', COMMIT + ':' + row['path']])
    assert pinned == (ORIGINAL / row['path']).read_bytes() == (SOURCE / row['path']).read_bytes() == (archive / row['path']).read_bytes()
    assert hashlib.sha256(pinned).hexdigest() == row['sha256']
for row in layout['installedDependencySources']:
    assert digest(DEPENDENCIES / row['path']) == row['sha256'], row['path']
assert digest(SOURCE / 'qa-server.mjs') == preparation['serverSHA256']
assert read('outputs/regressions/footer-layout-source-probe/index.json')['attempts'][1]['accepted'] is False

report = {
    'passed': True, 'sourceCommit': COMMIT, 'macOSFooterFontSamples': 40,
    'historicalChromeFontSamplesRetained': 40,
    'historicalChrome20Differences': differences,
    'componentStates': 6, 'componentFontSizes': [14, 20],
    'declaredDecoderCapabilities': 9,
    'limitedTwoDecoderSourceRetained': 'outputs/regressions/footer-macos-source-decoder-scope/correction.json',
    'pinnedFrontendFiles': len(layout['pinnedFrontendSources']),
    'dependencyFiles': len(layout['installedDependencySources']),
    'files': [{'path': name, 'sha256': digest(ROOT / name)} for name in (
        'tests/fixtures/footer-font-macos-source.json', 'tests/fixtures/value-footer-macos-source.json',
        font['rawReport'], layout['rawReport'], layout['preparation'], layout['execution'],
        'tests/fixtures/footer-font-source.json', 'scripts/check-footer-macos-source.py')],
    'scope': 'Pinned original macOS DOM advances and six controlled component states only. Five historical Chrome differences and rejected probe states retained. GPUI scene, raster, native input and full parity acceptance are separate.',
}
(ROOT / 'outputs/footer-macos-source-check.json').write_text(json.dumps(report, ensure_ascii=False, indent=2) + '\n')
print('MACOS FOOTER SOURCE VERIFIED: 40 font samples, six component states, five retained Chrome differences; native/product acceptance separate')
