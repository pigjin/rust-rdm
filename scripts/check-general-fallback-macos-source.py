#!/usr/bin/env python3
"""Source-only selected-type/font geometry; no current GPUI acceptance."""
import hashlib
import json
import struct
import subprocess
from pathlib import Path

ROOT = Path(__file__).resolve().parents[1]
ORIGINAL = Path('/Users/zlsj/Projects/tiny-rdm')
SOURCE = Path('/private/tmp/rust-rdm-footer-source-qa')
DEPS = Path('/private/tmp/rust-rdm-footer-node-deps/node_modules')

def sha(path):
    return hashlib.sha256(path.read_bytes()).hexdigest()

fixture = json.loads((ROOT / 'tests/fixtures/general-fallback-macos-source.json').read_text())
archive = ROOT / fixture['archiveDirectory']
prep = json.loads((ROOT / fixture['preparation']).read_text())
execution = json.loads((ROOT / fixture['execution']).read_text())
assert fixture['sourceCommit'] == '4ddb94a3c55f3401dbc66870f0d21cf0d6203cbc'
assert fixture['sourceEngine'] == 'macOS WKWebView'
assert execution['passed'] and execution['exitCode'] == 0 and execution['allInputHashesUnchanged']
assert sha(ROOT / fixture['rawReport']) == fixture['rawSHA256'] == execution['stdoutSHA256'] == sha(archive / 'stdout-attempt1.json')
assert sha(archive / 'stderr-attempt1.log') == execution['stderrSHA256']
assert (archive / 'stderr-attempt1.log').read_bytes() == b''
assert fixture['records'] == json.loads((ROOT / fixture['rawReport']).read_text())['records']
assert len(fixture['records']) == len(prep['configurations']) == 4
for row in fixture['files']:
    assert sha(ROOT / row['path']) == row['sha256'], row['path']
for row in prep['inputs']:
    assert sha(archive / Path(row['path']).name) == row['sha256']
for row in fixture['pinnedFrontendSources']:
    data = subprocess.check_output(['git', '-C', str(ORIGINAL), 'show', fixture['sourceCommit'] + ':' + row['path']])
    assert data == (ORIGINAL / row['path']).read_bytes() == (SOURCE / row['path']).read_bytes() == (archive / row['path']).read_bytes()
    assert hashlib.sha256(data).hexdigest() == row['sha256']
for row in fixture['installedDependencySources']:
    assert sha(DEPS / row['path']) == sha(archive / 'dependencies' / row['path']) == row['sha256']
checks = []
for configuration, record in zip(prep['configurations'], fixture['records']):
    assert record['configuration'] == configuration
    assert not record['sourceEventsAreTrusted'] and not record['inputFocused']
    assert record['filter'] == '' and record['match'] == configuration['label'] and record['searchDisabled']
    styles = {value['id']: value for value in record['styles']}
    assert styles['selector']['fontSize'] == '14px' and styles['selector']['fontWeight'] == '500'
    assert styles['selector']['padding'] == '0px 10px' and styles['selector']['h'] == 28
    assert styles['content']['fontSize'] == '12px' and styles['content']['textOverflow'] == 'clip'
    assert styles['prefix']['maxWidth'] == '50%'
    assert styles['selectorContent']['text'] == record['selectedTypeLabel']
    assert styles['selector']['w'] == styles['selectorContent']['w'] + 20
    assert styles['header']['w'] == configuration['asideWidth'] - 1
    assert struct.unpack('>II', (archive / (record['name'] + '.png')).read_bytes()[16:24]) == (2048, 1536)
    assert json.loads((archive / (record['name'] + '.json')).read_text()) == record
    checks.append({'scene': record['name'], 'selectorWidth': styles['selector']['w'], 'matchedTagWidth': styles['tag']['w'], 'computedFamily': styles['selector']['fontFamily']})
from PIL import Image
crops=[]
for record in fixture['records']:
    image=Image.open(archive/(record['name']+'.png')).convert('RGBA')
    crops.append(image.crop((100,80,1500,160)).tobytes())
assert crops[0]==crops[1] and crops[0]!=crops[2] and crops[0]!=crops[3]
report = {'passed': True, 'sourceStates': 4, 'checks': checks,
          'missingFamilyHeaderExactlyMatchesTimes': True,
          'missingFamilyHeaderDiffersFromTimesNewRomanAndSystem': True,
          'physicalPixelCrop': [100,80,1500,160],
          'pinnedFrontendFiles':len(fixture['pinnedFrontendSources']),
          'dependencyFiles':len(fixture['installedDependencySources']),
          'scope':'Original hidden WKWebView component only. Full visible700px header pixels identify Times as missing-family fallback,including Chinese placeholder. Not current GPUI/native orwhole-app acceptance.'}
(ROOT/'outputs/general-fallback-macos-source-check.json').write_text(json.dumps(report,indent=2)+'\n')
print('ORIGINAL FALLBACK SOURCE PASS:4 states;missing-family header exactly matchesTimes,notTimesNewRoman/system')
