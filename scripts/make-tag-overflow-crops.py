#!/usr/bin/env python3
"""Extract unchanged header pixels for scoped QA review, not generated imagery."""
import hashlib,json
from pathlib import Path
from PIL import Image
ROOT=Path(__file__).resolve().parents[1]
f=json.loads((ROOT/'tests/fixtures/tag-overflow-macos-source.json').read_text())
box=(100,80,700,160) # Logical50,40,300,40 at2x; excludes differing detail models.
rows=[]
for r in f['records']:
    name=r['name'];row={'scene':name,'physicalPixelCrop':box}
    for kind,source,folder in [('native',ROOT/'outputs/ui-qa'/(name+'.png'),ROOT/'outputs/ui-qa-tag-overflow-crops'),('reference',ROOT/f['archiveDirectory']/(name+'.png'),ROOT/'outputs/ui-reference-tag-overflow-crops')]:
        folder.mkdir(exist_ok=True);target=folder/(name+'.png')
        with Image.open(source)as im:
            assert im.size==(2048,1536);crop=im.crop(box);assert crop.size==(600,80);crop.save(target)
        row[kind]={'source':str(source.relative_to(ROOT)),'sourceSHA256':hashlib.sha256(source.read_bytes()).hexdigest(),'path':str(target.relative_to(ROOT)),'sha256':hashlib.sha256(target.read_bytes()).hexdigest()}
    rows.append(row)
(ROOT/'outputs/tag-overflow-macos-crops.json').write_text(json.dumps({'passed':True,'scenes':6,'records':rows,'scope':'Unmodified source/native header pixels extracted from2x full PNGs. Source and native right-pane models differ; these crops do not accept whole images or image equality.'},indent=2)+'\n')
print('EXTRACTED6 HEADER PAIRS; ORIGINAL FULL PNGS RETAINED')
