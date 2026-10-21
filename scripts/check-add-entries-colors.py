#!/usr/bin/env python3
"""Check the Source dark selected-radio fill and white caption contract."""
import json
from pathlib import Path
from PIL import Image
root=Path(__file__).resolve().parents[1]
source=json.loads((root/'tests/fixtures/add-entries-source.json').read_text())
checks=[]
for name in ['add-entries-dark-hash','add-entries-dark-zset']:
    record=next(r for r in source['records'] if r['name']==name)
    selected=next(r for r in record['radios'] if r['checked'])['bounds']
    counts=[];fills=[]
    for folder in ['ui-reference','ui-qa']:
        image=Image.open(root/'outputs'/folder/(name+'.png')).convert('RGB')
        scale=image.width/1024
        box=(round((selected['x']+8)*scale),round((selected['y']+6)*scale),round((selected['x']+selected['width']-8)*scale),round((selected['y']+selected['height']-6)*scale))
        counts.append(sum(all(channel>=200 for channel in pixel) for pixel in image.crop(box).get_flattened_data()))
        fills.append(image.getpixel((round((selected['x']+5)*scale),round((selected['y']+5)*scale))))
    assert min(counts)>10,(name,counts)
    assert all(abs(a-b)<=1 for a,b in zip(*fills)),(name,fills)
    checks.append({'scene':name,'sourceWhiteCaptionPixels':counts[0],'nativeWhiteCaptionPixels':counts[1],'sourceFill':fills[0],'nativeFill':fills[1]})
(root/'outputs/add-entries-color-check.json').write_text(json.dumps({'scope':'Two selected dark radio captions and one inset fill point each; does not prove identical glyph rasterization or whole images.','checks':checks,'passed':True},indent=2)+'\n')
print('ADD ENTRIES COLOR PASS: two Source dark selected-radio captions and fills')
