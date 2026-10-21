#!/usr/bin/env python3
"""Verify the original matched/draft controls captured without a tooltip."""
import hashlib
import json
import struct
import subprocess
from pathlib import Path

ROOT=Path(__file__).resolve().parents[1]
fixture=json.loads((ROOT/'tests/fixtures/entry-controls-macos-source.json').read_text())
archive=ROOT/fixture['archiveDirectory']
prep=json.loads((ROOT/fixture['preparation']).read_text())
execution=json.loads((ROOT/fixture['execution']).read_text())
sha=lambda p:hashlib.sha256(p.read_bytes()).hexdigest()
assert execution['passed'] and execution['exitCode']==0 and execution['allInputHashesUnchanged']
assert fixture['rawSHA256']==sha(ROOT/fixture['rawReport'])==execution['stdoutSHA256']==sha(archive/'stdout-attempt1.json')
assert sha(archive/'stderr-attempt1.log')==execution['stderrSHA256'] and not (archive/'stderr-attempt1.log').read_bytes()
assert len(fixture['records'])==len(prep['configurations'])==4
assert fixture['records']==json.loads((ROOT/fixture['rawReport']).read_text())['records']
for row in fixture['files']:assert sha(ROOT/row['path'])==row['sha256'],row['path']
for row in prep['inputs']:assert sha(archive/Path(row['path']).name)==row['sha256']
for row in fixture['pinnedFrontendSources']:
    data=subprocess.check_output(['git','-C','/Users/zlsj/Projects/tiny-rdm','show',fixture['sourceCommit']+':'+row['path']])
    assert data==(archive/row['path']).read_bytes()==(Path('/Users/zlsj/Projects/tiny-rdm')/row['path']).read_bytes()==(Path('/private/tmp/rust-rdm-footer-source-qa')/row['path']).read_bytes()
    assert hashlib.sha256(data).hexdigest()==row['sha256']
for row in fixture['installedDependencySources']:
    assert sha(archive/'dependencies'/row['path'])==sha(Path('/private/tmp/rust-rdm-footer-node-deps/node_modules')/row['path'])==row['sha256']
checks=[]
for row,c in zip(fixture['records'],prep['configurations']):
    assert row['configuration']==c and row['inputValue']==c['draft'] and row['fullMatch']==c['label']
    assert not row['tooltipVisible'] and not row['inputFocused'] and not row['sourceEventsAreTrusted']
    assert row['searchDisabled']==(c['draft']=='')
    styles={v['id']:v for v in row['styles']}
    assert styles['outer']['borderRadius']=='4px 0px 0px 4px'
    assert styles['search']['borderRadius']=='0px 4px 4px 0px' and styles['search']['lineHeight']=='14px'
    assert styles['search']['opacity']==('1'if c['draft']else'0.38'if c['theme']=='dark'else'0.5')
    assert struct.unpack('>II',(archive/(row['name']+'.png')).read_bytes()[16:24])==(2048,1536)
    assert json.loads((archive/(row['name']+'.json')).read_text())==row
    checks.append({'scene':row['name'],'disabled':row['searchDisabled'],'inputBorder':styles['inputBorder']['border'],'buttonBorder':styles['buttonBorder']['border'],'buttonText':styles['search']['color'],'buttonOpacity':styles['search']['opacity']})
report={'passed':True,'sourceStates':4,'checks':checks,'pinnedFrontendFiles':len(fixture['pinnedFrontendSources']),'dependencyFiles':len(fixture['installedDependencySources']),'scope':'Original actual WK control states, normal/disabled CSS and four raw PNGs with tooltip absent. No native/currentGPUI/hover/pressed or complete-image parity acceptance.'}
(ROOT/'outputs/entry-controls-macos-source-check.json').write_text(json.dumps(report,ensure_ascii=False,indent=2)+'\n')
print('ORIGINAL ENTRY CONTROLS SOURCE PASS:4 states, matched/draft, no tooltip, pinned component/theme sources')
