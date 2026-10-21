#!/usr/bin/env python3
"""Sample application-owned colors in the original and GPUI editor panels."""
import json
from pathlib import Path
from PIL import Image

root=Path(__file__).resolve().parents[1]
fixture=json.loads((root/'tests/fixtures/edit-entry-layout-source.json').read_text())
checks=[]
for sample in fixture['records']:
    images=[Image.open(root/'outputs'/folder/(sample['name']+'.png')).convert('RGB') for folder in ['ui-reference','ui-qa']]
    controls=sample['controls']
    points={
        'card':(controls['card']['x']+7,controls['card']['y']+7),
        'actions':(controls['actions']['x']+4,controls['actions']['y']+2),
        'input':(controls['input']['x']+controls['input']['width']-8,controls['input']['y']+16),
        'editor':(controls['editor']['x']+controls['editor']['width']-15,controls['editor']['y']+controls['editor']['height']-15),
        'save':(controls['save']['x']+5,controls['save']['y']+16),
        'pin':(controls['pin']['x']+2,controls['pin']['y']+13),
    }
    for name,(x,y) in points.items():
        rgb=[image.getpixel((round(x*2),round(y*2))) for image in images]
        delta=max(abs(a-b) for a,b in zip(*rgb))
        checks.append({'scene':sample['name'],'sample':name,'point':[x,y],'source':rgb[0],'native':rgb[1],'maxChannelDelta':delta,'passed':delta<=1})
result={'scope':'Six background/control points in thirteen settled-theme editor panels; RGB tolerance one. Excludes glyph rasterization, caret/ruler details, shadows, full-image equality, other fonts/locales and unrecorded interactions.','checks':checks,'passed':all(check['passed'] for check in checks)}
(root/'outputs/entry-panel-color-check.json').write_text(json.dumps(result,indent=2)+'\n')
for check in checks:
    if not check['passed']:print(check)
assert result['passed'],'editor panel color comparison failed'
print('ENTRY PANEL COLOR PASS:',len(checks),'Source background/control points')
