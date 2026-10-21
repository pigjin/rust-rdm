#!/usr/bin/env python3
"""Record provenance for the unchanged original NewKey desktop components."""
import hashlib
import json
from pathlib import Path
import shutil
import subprocess

ROOT=Path(__file__).resolve().parents[1]
SOURCE=Path('/Users/zlsj/Projects/tiny-rdm')
COPY=Path('/private/tmp/rust-rdm-source-qa')
COMMIT='4ddb94a3c55f3401dbc66870f0d21cf0d6203cbc'
assert subprocess.check_output(['git','rev-parse','HEAD'],cwd=SOURCE,text=True).strip()==COMMIT
assert not subprocess.check_output(['git','diff','HEAD','--stat'],cwd=SOURCE,text=True)
untracked=subprocess.check_output(['git','ls-files','--others','--exclude-standard'],cwd=SOURCE,text=True).splitlines()
paths=['src/components/dialogs/NewKeyDialog.vue']+['src/components/new_value/New'+kind+'Value.vue' for kind in ['String','Hash','List','Set','ZSet','Stream','Json']]
files=[]
for path in paths:
    original=(SOURCE/'frontend'/path).read_bytes()
    assert original==subprocess.check_output(['git','show',COMMIT+':frontend/'+path],cwd=SOURCE)
    assert original==(COPY/path).read_bytes(),path
    files.append({'path':path,'sha256':hashlib.sha256(original).hexdigest(),'unchanged':True})
frontend=json.loads((ROOT/'tests/fixtures/new-key-source.json').read_text())
backend=json.loads((ROOT/'tests/fixtures/new-key-backend-source.json').read_text())
assert len(frontend['records'])==19 and len(frontend['checks'])==24
assert len(backend['records'])==36
assert backend['sourceSha256']==hashlib.sha256((SOURCE/'backend/services/browser_service.go').read_bytes()).hexdigest()
assert all(case['request']['format']=='Raw' and case['request']['decode']=='None' for case in frontend['checks'] if case.get('request'))
assert next(case for case in frontend['records'] if case['name']=='new-key-value-resized')['inputs'][-1]['control']['height']==217
assert next(case for case in frontend['records'] if case['name']=='new-key-value-minimum')['inputs'][-1]['control']['height']==32
for name in ['qa-new-key.mjs','qa-new-key-resize.mjs']:
    shutil.copyfile(COPY/name,ROOT/'outputs/source-probes'/name.replace('qa-','',1))
for record in frontend['records']:
    shutil.copyfile(Path('/private/tmp/rust-rdm-new-key-reference')/(record['name']+'.png'),ROOT/'outputs/ui-reference'/(record['name']+'.png'))
report={'sourceCommit':COMMIT,'originalTrackedCheckoutClean':True,'untrackedFiles':untracked,'unchangedComponents':files,'frontendScenes':19,'frontendChecks':24,'backendCases':36,'backendExtraction':backend['method'],'scope':'Local source UI / API boundary and actual Redis backend samples. Does not prove native Wails input, OS IME, animations, every locale or all key operations.'}
(ROOT/'outputs/new-key-source-check.json').write_text(json.dumps(report,ensure_ascii=False,indent=2)+'\n')
print('NEW KEY SOURCE CHECK PASS: 8 unchanged components, 19 UI scenes, 24 frontend checks, 36 backend cases')
