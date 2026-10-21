#!/usr/bin/env python3
"""Validate bounded original WebKit weight evidence, not whole UI parity."""
import hashlib
import json
import subprocess
from pathlib import Path
ROOT = Path(__file__).resolve().parents[1]
def read(name):
    return json.loads((ROOT / name).read_text())
def digest(name):
    return hashlib.sha256((ROOT / name).read_bytes()).hexdigest()
f = read('tests/fixtures/font-weight-macos-source.json')
r = read(f['rawReport'])
v = read('outputs/system-font-weight-variation-raw.json')
assert digest(f['rawReport']) == f['rawReportSHA256']
assert len(f['samples']) == len(r['source']['samples']) == len(r['native']) == 252
expected = [{'fontSize':x['size'],'weight':x['weight'],'text':x['text'],'width':x['width']} for x in r['source']['samples']]
assert f['samples'] == expected
mixed=[{'fontSize':x['size'],'runs':x['runs'],'width':x['width']}for x in r['source']['mixedSamples']]
assert len(mixed)==14 and f['mixedSamples']==mixed
optical=read('outputs/system-font-weight-optical-expanded-raw.json')
controls=[x for x in optical['rows']if x['mode']=='UIFont16-weight-f32-nextUp-point-optical']
assert len(controls)==252
control_matches=[]
control_failures=[]
for row in controls:
    source=next(x for x in expected if (x['fontSize'],x['weight'],x['text'])==(row['size'],row['weight'],row['text']))
    (control_matches if abs(row['width']-source['width'])<0.02 else control_failures).append({'fontSize':row['size'],'weight':row['weight'],'text':row['text'],'nativeWidth':row['width'],'sourceWidth':source['width']})
assert len(control_matches)==234 and len(control_failures)==18
assert {(x['fontSize'],x['text'])for x in control_failures}=={(24,'中文🙂 alpha'),(32,'中文🙂 alpha')}
corrected=read('outputs/system-font-weight-fallback-cg-raw.json')
assert len(corrected['rows'])==252
for row in corrected['rows']:
    source=next(x for x in expected if (x['fontSize'],x['weight'],x['text'])==(row['size'],row['weight'],row['text']))
    assert abs(row['width']-source['width'])<0.02,(row,source)
negative=[x for x in optical['rows']if x['mode']=='UIFont16-weight-f32-nextUp' and abs(x['width']-next(s['width']for s in expected if(s['fontSize'],s['weight'],s['text'])==(x['size'],x['weight'],x['text'])))>=0.02]
assert len(negative)>0
assert {x['fontSize'] for x in expected} == {12,13,14,15,18,24,32}
assert {x['weight'] for x in expected} == set(range(100,901,100))
assert {x['text'] for x in expected} == {'localhost','replica','Tiny RDM','中文🙂 alpha'}
assert f['fontFamily'] == r['source']['family']
assert not any(x['family'] == 'v-sans' for x in r['source']['fonts'])
for row in f['files']:
    assert digest(row['path']) == row['sha256'], row['path']
style = ROOT/'outputs/source-probes/system-font-weight/frontend/src/styles/style.scss'
pinned = subprocess.run(['git','show',f["sourceCommit"]+':frontend/src/styles/style.scss'],cwd='/Users/zlsj/Projects/tiny-rdm',check=True,capture_output=True).stdout
assert style.read_bytes() == pinned
matrix = []
for weight in range(100,901,100):
    source = next(x for x in expected if x['fontSize']==14 and x['weight']==weight and x['text']=='localhost')
    sample = next(x for x in v['rows'] if x['label']=='CTFont system wght='+str(float(weight)))
    assert abs(sample['width']-source['width']) < 0.02
    if weight != 400:
        assert sample['variation']['2003265652'] == weight
    matrix.append({'weight':weight,'originalDOMWidth':source['width'],'nativeAxisWidth':sample['width'],'postscript':sample['postscript']})
assert len({x['postscript'] for x in matrix}) == 1
for name,axis in [('heavy',860),('black',1000)]:
    row = next(x for x in v['rows'] if x['label']==name)
    assert row['variation']['2003265652'] == axis
files = ['tests/fixtures/font-weight-macos-source.json',f['rawReport'],'outputs/system-font-weight-variation-raw.json','outputs/system-font-weight-source-preparation.json','outputs/system-font-weight-source-execution.json','outputs/system-font-weight-expanded-mixed-source-execution.json','outputs/system-font-weight-optical-expanded-raw.json','outputs/system-font-weight-fallback-cg-raw.json','scripts/check-font-weight-source.py','vendor/gpui-apple/src/text_system.rs','vendor/gpui-apple/RDM-CHANGES.md']
report = {'sourceCommit':f['sourceCommit'],'sourceDOMSamples':252,'sourceMixedDOMSamples':14,'nativeNamedSamples':252,'nativeOpticalControlSamples':252,'nativeOpticalMatches':len(control_matches),'nativeOpticalFailures':control_failures,'nativeOpticalAllMatch':False,'correctedNativeFallbackSamples':252,'correctedNativeFallbackAllMatch':True,'nativeUnfixedNegativeSamples':len(negative),'nativeAxisHostSamples':9,'nativeAxisHostChecks':matrix,'files':[{'path':name,'sha256':digest(name)} for name in files],'scope':'Original body CSS DOM advances and separate public native font-axis measurement only. Source evidence integrity passed; 18 larger Unicode native controls remain explicitly failed, not accepted or removed. GPUI widths, glyph instances, scenes, bitmaps and native acceptance are reported separately. The failed first native variation probe is archived as failed; it contributed no accepted data.','passed':True}
(ROOT/'outputs/font-weight-source-check.json').write_text(json.dumps(report,ensure_ascii=False,indent=2)+'\n')
print('MACOS FONT WEIGHT SOURCE PASS: 252 single-font +14 mixed-weight original DOM samples; 234 matching native optical controls, 18 retained negative controls, 252 corrected fallback controls, nine axis controls; pinned CSS and probe hashes verified')
