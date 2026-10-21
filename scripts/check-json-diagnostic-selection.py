#!/usr/bin/env python3
"""Bounded macOS diagnostic selection comparisons against original Monaco.

Checks selection fill, extent and suffix foreground, not complete-image equality.
"""
import hashlib
import json
from pathlib import Path
from PIL import Image

ROOT=Path(__file__).resolve().parents[1]
SCENES={
 'entry-diagnostic-select-partial':'hover-partial',
 'entry-diagnostic-select-whole':'hover-whole',
 'entry-diagnostic-select-code':'hover-code-span',
 'entry-diagnostic-select-wrapped':'hover-wrapped-whole',
 'entry-diagnostic-select-overlap':'hover-overlap-whole',
 'entry-diagnostic-select-reverse':'hover-overlap-reverse',
 'entry-diagnostic-select-word':'hover-word-double-click',
 'entry-diagnostic-select-navigation':'navigation-partial',
 'entry-diagnostic-select-unicode':'controlled-unicode-hard-line',
}
fixture=ROOT/'tests/fixtures/json-diagnostic-selection-source.json'
source=json.loads(fixture.read_text())
assert source['sourceCommit']=='4ddb94a3c55f3401dbc66870f0d21cf0d6203cbc'
assert len(source['records'])==10 and len(source['focusEvents'])==3
for record in source['records']:
 assert record['copyEvent']['selected']==record['selected']
 assert not record['editorTextFocused'] and not record['copyEvent']['providedText']
 assert record['gesture']['kind'] in ['drag','double-click']
assert source['focusEvents'][0]['editorTextFocused'] and not source['focusEvents'][0]['hoverVisible']
assert source['focusEvents'][1]['editorTextFocused'] and '2 of 2 problems' in source['focusEvents'][1]['navigationText']
assert source['focusEvents'][2]['editorTextFocused'] and not source['focusEvents'][2]['navigationVisible']

checks=[]
def mask(image,color):
 points=[(x,y)for y in range(360,840)for x in range(1400,2018)
         if max(abs(a-b)for a,b in zip(image.getpixel((x,y)),color))<=1]
 return {'pixels':len(points),'bounds':[min(x for x,y in points),min(y for x,y in points),max(x for x,y in points),max(y for x,y in points)]if points else None}
for scene,name in SCENES.items():
 paths=[ROOT/'outputs/ui-reference'/('json-select-'+name+'.png'),ROOT/'outputs/ui-qa'/(scene+'.png')]
 images=[Image.open(p).convert('RGB')for p in paths]
 assert all(im.size==(2048,1536)for im in images)
 colors=[(179,215,254)]if name.startswith('navigation')else[(174,210,250),(202,223,247)]
 for color in colors:
  masks=[mask(im,color)for im in images]
  if not masks[0]['bounds']:continue
  delta=max(abs(a-b)for a,b in zip(masks[0]['bounds'],masks[1]['bounds']))if masks[1]['bounds']else None
  checks.append({'scene':scene,'sample':'selection fill extent','expectedRGB':color,'masks':masks,'maxExtentDelta':delta,'passed':delta is not None and delta<=2})
 if name in ['hover-whole','hover-code-span']:
  masks=[mask(im,(155,155,155))for im in images]
  checks.append({'scene':scene,'sample':'opaque result of suffix opacity group','expectedRGB':[155,155,155],'masks':masks,'passed':all(m['pixels']>100 for m in masks)})
report={'scope':'Nine original/native selection image pairs; bounded macOS selection colors and extents within two physical pixels, plus two suffix foreground checks. Does not prove complete glyph/antialiasing equality, other operating systems, or actual native-window selection.',
 'sourceFixtureSha256':hashlib.sha256(fixture.read_bytes()).hexdigest(),'sourceRecords':10,'pairedFrames':9,'focusEvents':3,
 'checks':checks,'fullVisualParityComplete':False,'passed':all(c['passed']for c in checks)}
(ROOT/'outputs/json-diagnostic-selection-color-check.json').write_text(json.dumps(report,indent=2)+'\n')
for check in checks:
 if not check['passed']:print(check)
assert report['passed'],'diagnostic selection pixel comparison failed'
print('DIAGNOSTIC SELECTION PIXEL PASS:',len(checks),'bounded checks across nine pairs')
