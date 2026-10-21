#!/usr/bin/env python3
"""Bounded pixel checks for ten original/native diagnostic surface pairs.

Checks fills, foregrounds, navigation frame/arrow/icon extents and scrollbar.
Does not claim full glyph, antialiasing, caret or complete-image equivalence.
"""
import json
from pathlib import Path
from PIL import Image

ROOT = Path(__file__).resolve().parents[1]
SCENES = {
    'entry-diagnostic-hover': 'json-diagnostic-hover',
    'entry-diagnostic-hover-dark': 'json-diagnostic-hover-dark',
    'entry-diagnostic-hover-overlap': 'json-diagnostic-hover-overlap',
    'entry-diagnostic-hover-overlap-dark': 'json-diagnostic-hover-overlap-dark',
    'entry-diagnostic-hover-full': 'json-hover-full',
    'entry-diagnostic-hover-short': 'json-hover-short',
    'entry-diagnostic-hover-duplicate': 'json-hover-warning',
    'entry-diagnostic-hover-overlap-full': 'json-hover-overlap-full',
    'entry-diagnostic-navigation': 'json-diagnostic-navigation',
    'entry-diagnostic-navigation-dark': 'json-diagnostic-navigation-dark',
}
records = {}
for name in ['json-diagnostic-hover-source', 'json-diagnostic-hover-interaction-source', 'json-hover-layout-source']:
    records.update((record['name'], record) for record in json.loads((ROOT / f'tests/fixtures/{name}.json').read_text())['records'])

checks = []

def mask(image, bounds, color):
    left, top, right, bottom = [round(value * 2) for value in bounds]
    points = [(x, y) for y in range(top, bottom) for x in range(left, right)
              if max(abs(a-b) for a, b in zip(image.getpixel((x, y)), color)) <= 1]
    return {'pixels': len(points), 'bounds': [min(p[0] for p in points), min(p[1] for p in points), max(p[0] for p in points), max(p[1] for p in points)] if points else None}

def extent(scene, name, pictures, bounds, color, tolerance=2):
    masks = [mask(picture, bounds, color) for picture in pictures]
    delta = max(abs(a-b) for a, b in zip(masks[0]['bounds'], masks[1]['bounds'])) if all(m['bounds'] for m in masks) else None
    checks.append({'scene': scene, 'sample': name, 'expectedRGB': color, 'masks': masks,
                   'maxExtentDelta': delta, 'passed': delta is not None and delta <= tolerance})

for scene, source in SCENES.items():
    record = records[source]
    pictures = [Image.open(ROOT / 'outputs' / folder / (name + '.png')).convert('RGB')
                for folder, name in [('ui-reference', source), ('ui-qa', scene)]]
    assert all(picture.size == (2048, 1536) for picture in pictures)
    dark = scene.endswith('-dark')
    box = record.get('hover', record.get('bounds'))
    x, y, width, height = [box[key] for key in ['x', 'y', 'width', 'height']]
    if 'hover' in scene:
        for label, px, py, expected in [
            ('body fill', x+5, y+3, (37,37,38) if dark else (243,243,243)),
            ('footer fill', x+width-5, y+height-4, (44,44,45) if dark else (231,231,231)),
        ]:
            colors = [picture.getpixel((round(px*2), round(py*2))) for picture in pictures]
            checks.append({'scene': scene, 'sample': label, 'point': [px, py], 'expectedRGB': expected,
                           'source': colors[0], 'native': colors[1],
                           'passed': all(max(abs(a-b) for a,b in zip(color,expected)) <= 1 for color in colors)})
        extent(scene, 'border extent', pictures, (x-1,y-1,x+width+1,y+height+1), (69,69,69) if dark else (200,200,200))
        foreground = (204,204,204) if dark else (97,97,97)
        masks = [mask(picture, (x+8,y+4,x+width-6,y+height-24),foreground) for picture in pictures]
        checks.append({'scene':scene,'sample':'body foreground','expectedRGB':foreground,'masks':masks,'passed':all(m['pixels']>30 for m in masks)})
    else:
        frame = (241,76,76) if dark else (229,20,0)
        extent(scene, 'frame extent', pictures, (x-1,y+7,x+width+1,y+height), frame)
        extent(scene, 'arrow extent', pictures, (x+20,y,x+70,y+7), frame)
        metrics = record['metrics']
        for css in ['codicon codicon codicon-error', 'action-label codicon codicon-marker-navigation-next', 'action-label codicon codicon-marker-navigation-previous', 'action-label codicon codicon-close']:
            item = next(m for m in metrics if m['class'] == css)
            icon = item['bounds']; ix,iy,iw,ih=[icon[key] for key in ['x','y','width','height']]
            extent(scene, css.rsplit('codicon-',1)[-1]+' icon extent',pictures,(ix-1,iy-1,ix+iw+1,iy+ih+1),frame if css.endswith('error') else ((212,212,212) if dark else (0,0,0)))
        slider = next(m for m in metrics if m['class'] == 'slider')['bounds']
        sx,sy,sw,sh=[slider[key] for key in ['x','y','width','height']]
        extent(scene,'scrollbar thumb extent',pictures,(sx-1,sy-1,sx+sw+1,sy+sh+1),(66,66,66) if dark else (193,193,192))

report = {'scope':'Ten original/native diagnostic surface pairs. Bounded solid fills, foreground masks and selected extents only; two physical pixels extent tolerance. Full-image, text glyph and antialiasing equivalence are excluded.',
          'checks':checks,'fullVisualParityComplete':False,'passed':all(check['passed'] for check in checks)}
(ROOT / 'outputs/json-diagnostic-surfaces-color-check.json').write_text(json.dumps(report,indent=2)+'\n')
for check in checks:
    if not check['passed']: print(check)
assert report['passed'], 'diagnostic surface pixel comparison failed'
print('DIAGNOSTIC SURFACE PIXEL PASS:',len(checks),'bounded checks across ten pairs')
