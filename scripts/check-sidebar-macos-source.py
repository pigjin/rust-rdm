#!/usr/bin/env python3
"""Pinned original sidebar drag/layout and settled divider-state evidence."""
import hashlib,json,struct,subprocess
from pathlib import Path
ROOT=Path(__file__).resolve().parents[1]
ORIGINAL=Path('/Users/zlsj/Projects/tiny-rdm')
SOURCE=Path('/private/tmp/rust-rdm-footer-source-qa')
DEPS=Path('/private/tmp/rust-rdm-footer-node-deps/node_modules')
def sha(p):return hashlib.sha256(p.read_bytes()).hexdigest()
counts=[]
for filename in ['sidebar-resize-macos-source.json','sidebar-contract-macos-source.json']:
 f=json.loads((ROOT/'tests/fixtures'/filename).read_text());a=ROOT/f['archiveDirectory'];p=json.loads((ROOT/f['preparation']).read_text());e=json.loads((ROOT/f['execution']).read_text())
 assert f['sourceCommit']=='4ddb94a3c55f3401dbc66870f0d21cf0d6203cbc'and f['sourceEngine']=='macOS WKWebView'
 assert e['passed']and e['exitCode']==0 and e['allInputHashesUnchanged']
 assert sha(ROOT/f['rawReport'])==f['rawSHA256']==e['stdoutSHA256']==sha(a/'stdout-attempt1.json')
 assert sha(a/'stderr-attempt1.log')==e['stderrSHA256']and(a/'stderr-attempt1.log').read_bytes()==b''
 assert f['records']==json.loads((ROOT/f['rawReport']).read_text())['records']and len(f['records'])==8
 for v in f['files']:assert sha(ROOT/v['path'])==v['sha256'],v['path']
 for v in p['inputs']:assert sha(a/Path(v['path']).name)==v['sha256']
 for v in f['pinnedFrontendSources']:
  data=subprocess.check_output(['git','-C',str(ORIGINAL),'show',f['sourceCommit']+':'+v['path']]);assert data==(ORIGINAL/v['path']).read_bytes()==(SOURCE/v['path']).read_bytes()==(a/v['path']).read_bytes();assert hashlib.sha256(data).hexdigest()==v['sha256']
 for v in f['installedDependencySources']:assert sha(DEPS/v['path'])==sha(a/'dependencies'/v['path'])==v['sha256']
 for c,v in zip(p['configurations'],f['records']):
  assert c==v['configuration']and v['match']==c['label']and v['filter']==''and v['searchDisabled']and not v['inputFocused']and v['sourceEventsAreTrusted']is False
  m={z['id']:z for z in v['styles']};width=max(300,c['requestedWidth']);assert v['actualSidebarWidth']==m['sidebar']['w']==width
  assert m['sidebar']['x']==51 and m['sidebar']['y']==40 and m['sidebar']['h']==727 and m['sidebar']['overflow']=='visible'
  assert m['divider']['w']==4 and m['divider']['h']==727 and m['divider']['x']==49+width and m['divider']['y']==40
  assert m['header']['w']==width-1 and m['prefix']['maxWidth']=='50%'and m['content']['textOverflow']=='clip'
  if 'mode'in c:
   active=c['mode']in ['pressed','dragging'];assert v['resizing']==active and v['hovering']==(c['mode']!='overflow')
   color='rgb(211, 58, 49)'if active else('rgb(81, 81, 81)'if c['theme']=='dark'else'rgb(224, 224, 230)')if c['mode']=='hover'else'rgba(0, 0, 0, 0)'
   assert m['divider']['background']==color and 'background-color 0.3s ease-in'in m['divider']['transition']
  assert struct.unpack('>II',(a/(v['name']+'.png')).read_bytes()[16:24])==(2048,1536)
  assert json.loads((a/(v['name']+'.json')).read_text())==v
 counts.append({'fixture':filename,'states':8,'pinnedFrontendFiles':len(f['pinnedFrontendSources']),'dependencyFiles':len(f['installedDependencySources'])})
source=(ORIGINAL/'frontend/src/AppContent.vue').read_text();assert 'debounce(prefStore.savePreferences, 1000, { trailing: true })'in source and ':offset="data.navMenuWidth"'in source
source=(ORIGINAL/'frontend/src/components/common/ResizeableWrapper.vue').read_text();assert 'evt.clientX - props.offset'in source and 'props.maxSize > 0'in source
report={'passed':True,'sourceStates':16,'groups':counts,'sidebarMinimum':300,'outsideViewportWidthObserved':1200,'dividerWidth':4,'dividerRightOverhang':2,'sourceSaveDebounceMillisecondsFromCode':1000,'sourceTransition':'background-color300ms ease-in','sourceEventsAreTrusted':False,'scope':'Original bytes/DOM/PNG and programmatic component/document operations only. Save delay is source-code proof,not measured original disk timing. Settled divider colors,not intermediate animation-frame equality. No current GPUI/hardware/native/full-image acceptance.'}
(ROOT/'outputs/sidebar-macos-source-check.json').write_text(json.dumps(report,indent=2)+'\n');print('ORIGINAL SIDEBAR SOURCE PASS:16 states,300 minimum,1200 outsideviewport,4px band,settled colors and1000ms source save contract')
