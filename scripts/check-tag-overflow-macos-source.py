#!/usr/bin/env python3
"""Original long-tag clipping proof; does not accept current GPUI rendering."""
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
def sha(p): return hashlib.sha256(p.read_bytes()).hexdigest()
def xml(n): return [n.tag.rsplit('}', 1)[-1], dict(n.attrib), [xml(c) for c in n]]
f=json.loads((ROOT/'tests/fixtures/tag-overflow-macos-source.json').read_text())
a=ROOT/f['archiveDirectory'];p=json.loads((ROOT/f['preparation']).read_text());e=json.loads((ROOT/f['execution']).read_text())
assert f['sourceEngine']=='macOS WKWebView' and f['sourceCommit']=='4ddb94a3c55f3401dbc66870f0d21cf0d6203cbc'
assert e['passed'] and e['exitCode']==0 and e['allInputHashesUnchanged']
assert sha(ROOT/f['rawReport'])==f['rawSHA256']==e['stdoutSHA256']==sha(a/'stdout-attempt1.json')
assert sha(a/'stderr-attempt1.log')==e['stderrSHA256'] and (a/'stderr-attempt1.log').read_bytes()==b''
assert f['records']==json.loads((ROOT/f['rawReport']).read_text())['records'] and len(f['records'])==6
for v in f['files']: assert sha(ROOT/v['path'])==v['sha256'],v['path']
for v in p['inputs']: assert sha(a/Path(v['path']).name)==v['sha256']
for v in f['pinnedFrontendSources']:
 data=subprocess.check_output(['git','-C',str(ORIGINAL),'show',f['sourceCommit']+':'+v['path']])
 assert data==(ORIGINAL/v['path']).read_bytes()==(SOURCE/v['path']).read_bytes()==(a/v['path']).read_bytes()
 assert hashlib.sha256(data).hexdigest()==v['sha256']
for v in f['installedDependencySources']: assert sha(DEPS/v['path'])==sha(a/'dependencies'/v['path'])==v['sha256']
for c,v in zip(p['configurations'],f['records']):
 assert c==v['configuration'] and v['name']==c['name'] and v['match']==c['label'] and v['filter']==''
 assert v['inputFocused'] and v['searchDisabled'] and v['sourceEventsAreTrusted'] is False
 m={s['id']:s for s in v['styles']}
 assert m['prefix']['maxWidth']=='50%' and m['wrapper']['padding']=='0px 3px'
 assert m['prefix']['w']==(m['wrapper']['w']-6)/2==m['tag']['w']==72.21875
 assert m['content']['w']==m['tag']['w']-30==42.21875
 assert m['content']['overflow']=='hidden' and m['content']['textOverflow']=='clip' and m['content']['fontSize']=='12px'
 assert m['tag']['h']==22 and m['close']['w']==m['close']['h']==12 and m['closeIcon']['w']==m['closeIcon']['h']==16
 assert xml(ET.fromstring(m['closeIcon']['svg']))==xml(ET.fromstring((ROOT/'assets/icons/search-match-close.svg').read_text()))
 assert struct.unpack('>II',(a/(v['name']+'.png')).read_bytes()[16:24])==(2048,1536)
 assert json.loads((a/(v['name']+'.json')).read_text())==v
report={'passed':True,'sourceStates':6,'pinnedFrontendFiles':len(f['pinnedFrontendSources']),'dependencyFiles':len(f['installedDependencySources']),'tagMaximumWidthAtDefault300Sidebar':72.21875,'labelMaximumWidth':42.21875,'tagPointSize':12,'sourceTextOverflow':'clip','sourcePrefixMaximum':'50% of input wrapper content width','themes':['light','dark'],'generalPointSizes':[14,20],'sourceEventsAreTrusted':False,'scope':'Pinned original bytes/DOM/PNG/state integrity only; default300 sidebar,ASCII/Chinese/Emoji labels. No GPUI,actual Redis,native or whole-image parity acceptance.'}
(ROOT/'outputs/tag-overflow-macos-source-check.json').write_text(json.dumps(report,indent=2)+'\n')
print('ORIGINAL LONG TAG SOURCE PASS:6 states,72.21875px max,clip without ellipsis')
