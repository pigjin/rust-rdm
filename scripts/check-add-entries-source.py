#!/usr/bin/env python3
import hashlib,json,shutil,subprocess
from pathlib import Path
root=Path(__file__).resolve().parents[1];source=Path('/Users/zlsj/Projects/tiny-rdm');copy=Path('/private/tmp/rust-rdm-source-qa')
commit='4ddb94a3c55f3401dbc66870f0d21cf0d6203cbc'
assert subprocess.check_output(['git','rev-parse','HEAD'],cwd=source,text=True).strip()==commit
assert not subprocess.check_output(['git','diff','HEAD','--stat'],cwd=source,text=True)
paths=['src/components/dialogs/AddFieldsDialog.vue']+['src/components/new_value/'+name+'.vue' for name in ['AddHashValue','AddListValue','NewSetValue','AddZSetValue','NewStreamValue']]
files=[]
for path in paths:
 data=(source/'frontend'/path).read_bytes();assert data==(copy/path).read_bytes();assert data==subprocess.check_output(['git','show',commit+':frontend/'+path],cwd=source)
 files.append({'path':path,'sha256':hashlib.sha256(data).hexdigest(),'unchanged':True})
ui=json.loads((root/'tests/fixtures/add-entries-source.json').read_text());backend=json.loads((root/'tests/fixtures/add-entries-backend-source.json').read_text())
assert len(ui['records'])==16 and len(ui['checks'])==24 and len(backend['records'])==35
assert backend['sourceSha256']==hashlib.sha256((source/'backend/services/browser_service.go').read_bytes()).hexdigest()
for record in ui['records']:shutil.copyfile(Path('/private/tmp/rust-rdm-add-entries-reference')/(record['name']+'.png'),root/'outputs/ui-reference'/(record['name']+'.png'))
shutil.copyfile(copy/'qa-add-entries.mjs',root/'outputs/source-probes/add-entries-source.mjs')
shutil.copyfile(copy/'qa-add-entries-race.mjs',root/'outputs/source-probes/add-entries-race-source.mjs')
report={'sourceCommit':commit,'unchangedComponents':files,'uiScenes':16,'frontendChecks':24,'backendCases':35,'backendMethods':backend['methodSha256'],'scope':'Unchanged original components with local API boundary, plus five extracted Go methods and actual disposable Redis. Does not prove native Wails/IME, animations, every locale/font, IO interruption or all concurrent requests.','passed':True}
(root/'outputs/add-entries-source-check.json').write_text(json.dumps(report,ensure_ascii=False,indent=2)+'\n')
print('ADD ENTRIES SOURCE PASS: 6 components, 16 scenes, 24 frontend checks, 35 Redis cases')
