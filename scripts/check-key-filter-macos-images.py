#!/usr/bin/env python3
"""Bounded header fills/text crops/clear-icon extents; no full-image parity."""
import argparse
import hashlib
import json
import math
from collections import Counter
from pathlib import Path
from PIL import Image

ROOT = Path(__file__).resolve().parents[1]
p = argparse.ArgumentParser()
p.add_argument('--log', required=True)
args = p.parse_args()
log = Path(args.log)
frames = {}
for line in log.read_text().splitlines():
    if line.startswith('KEY FILTER MACOS FRAME: '):
        row = json.loads(line.split(': ', 1)[1])
        frames[row['scene']] = {m['id']: m for m in row['frames']}
f = json.loads((ROOT / 'tests/fixtures/key-filter-macos-source.json').read_text())
assert len(frames) == len(f['records']) == 22
checks = []
files = [log, Path(__file__), ROOT / 'tests/fixtures/key-filter-macos-source.json']

def sample(image, r, x, y):
    return image.getpixel((round((r['x']+x)*2), round((r['y']+y)*2)))

def ink(image, r, bg):
    x0, y0 = math.ceil(r['x']*2), math.ceil(r['y']*2)
    x1, y1 = math.floor((r['x']+r['w'])*2), math.floor((r['y']+r['h'])*2)
    points = [(x, y) for y in range(y0, y1) for x in range(x0, x1)
              if max(abs(a-b) for a,b in zip(image.getpixel((x,y)),bg)) > 20]
    if not points:
        return None
    return [min(x for x,y in points)-r['x']*2, min(y for x,y in points)-r['y']*2,
            max(x for x,y in points)-r['x']*2, max(y for x,y in points)-r['y']*2]

for row in f['records']:
    name = row['name']; m = {r['id']: r for r in row['metrics']}; actual = frames[name]
    paths = [ROOT / f['archiveDirectory'] / (name+'.png'), ROOT / 'outputs/ui-qa' / (name+'.png')]
    files.extend(paths)
    images = [Image.open(path).convert('RGB') for path in paths]
    assert all(im.size == (2048,1536) for im in images)
    for component, offset in [('header',(4,3)),('outer',(4,4)),('exact',(2,3)),('search',(3,3))]:
        if component == 'outer':
            leaf=m['input'];dx=leaf['x']-m['outer']['x']+leaf['w']-4
            colors=[sample(im,r,dx,3)for im,r in zip(images,[m['outer'],actual['outer']])]
        else:
            colors = [sample(im, r, *offset) for im,r in zip(images,[m[component],actual[component]])]
        delta = max(abs(a-b) for a,b in zip(*colors))
        checks.append({'scene':name,'component':component,'sample':'interior fill','colors':colors,'maximumRGBDelta':delta,'threshold':1,'passed':delta<=1})
    for component in ['exact','search']:
        rects=[m[component],actual[component]]
        crops=[dict(x=r['x']+4,y=r['y']+4,w=r['w']-8,h=r['h']-8)for r in rects]
        bgs=[sample(im,r,2 if component=='exact' else 3,3)for im,r in zip(images,rects)]
        masks=[ink(im,r,bg)for im,r,bg in zip(images,crops,bgs)]
        delta=max(abs(a-b)for a,b in zip(*masks))if all(masks)else None
        checks.append({'scene':name,'component':component,'sample':'interior icon extent','masks':masks,'maximumPhysicalPixelDelta':delta,'threshold':2,'passed':delta is not None and delta<=2})
    # Empty unfocused controls compare the complete placeholder crop. Focused
    # empty crops skip the leading caret; nonempty crops use20px so hash's trailing caret is
    # outside the crop. This verifies font/position, not caret or full glyphs.
    source_input = m['input']; outer = m['outer']; target = actual['outer']
    left = 2 if row['inputFocused'] and not row['filter'] else 0
    width = 20 if row['inputFocused'] else min(source_input['w'],80)
    crops = [{'x':source_input['x']+left,'y':source_input['y']+3,'w':width,'h':source_input['h']-6},
             {'x':target['x']+source_input['x']-outer['x']+left,'y':target['y']+3,'w':width,'h':source_input['h']-6}]
    backgrounds = [sample(im,r,source_input['x']-outer['x']+source_input['w']-4,3) for im,r in zip(images,[outer,target])]
    masks = [ink(im,r,bg)for im,r,bg in zip(images,crops,backgrounds)]
    delta = max(abs(a-b)for a,b in zip(*masks)) if all(masks)else None
    checks.append({'scene':name,'sample':'bounded input text extent','masks':masks,'maximumPhysicalPixelDelta':delta,'threshold':2,'passed':delta is not None and delta<=2})
    if row['filter'] and row['inputFocused']:
        rects = [m['clear'],actual['clear']]
        masks = [ink(im,r,bg)for im,r,bg in zip(images,rects,backgrounds)]
        delta = max(abs(a-b)for a,b in zip(*masks))if all(masks)else None
        checks.append({'scene':name,'sample':'clear icon extent','masks':masks,'maximumPhysicalPixelDelta':delta,'threshold':2,'passed':delta is not None and delta<=2})
        # Most frequent non-background color within the filled circle measures
        # its interior; antialiased edges and hover/pressed colors are separate.
        colors=[]
        for im,r,bg in zip(images,rects,backgrounds):
            crop=im.crop((round(r['x']*2),round(r['y']*2),round((r['x']+r['w'])*2),round((r['y']+r['h'])*2)))
            pixels=[c for c in crop.get_flattened_data()if max(abs(a-b)for a,b in zip(c,bg))>20]
            colors.append(Counter(pixels).most_common(1)[0][0] if pixels else None)
        delta=max(abs(a-b)for a,b in zip(*colors)) if all(colors) else None
        checks.append({'scene':name,'sample':'clear icon interior color','colors':colors,'maximumRGBDelta':delta,'threshold':1,'passed':delta is not None and delta<=1})
report={'passed':all(r['passed']for r in checks),'scenes':22,'checks':checks,'logSHA256':hashlib.sha256(log.read_bytes()).hexdigest(),
        'files':[{'path':str(p.relative_to(ROOT))if p.is_relative_to(ROOT)else str(p),'sha256':hashlib.sha256(p.read_bytes()).hexdigest()}for p in files],
        'scope':'Only header/input/button interior points, predetermined caret-excluding input text crops, clear-icon extents/interior colors. RGB1 and unchanged2-physical-pixel extents. Fixed source model may differ outside header. No border/shadow, caret, full glyph bitmap, whole image or native acceptance.'}
(ROOT/'outputs/key-filter-macos-image-check.json').write_text(json.dumps(report,indent=2)+'\n')
bad=[r for r in checks if not r['passed']]
for r in bad[:20]:print(r)
assert report['passed'],f'{len(bad)} key-filter bounded image checks failed'
print('KEY FILTER MACOS IMAGES PASS:',len(checks),'bounded checks across22 states; full/native parity separate')
