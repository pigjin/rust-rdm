#!/usr/bin/env python3
"""Bounded tooltip fill/text/arrow/corner paint against original WK captures."""
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
p.add_argument('--native-dir', required=True)
p.add_argument('--include-entry-widths', action='store_true')
p.add_argument('--arrow-shadow', action='store_true')
p.add_argument('--output', default='outputs/match-tooltip-macos-image-check.json')
args = p.parse_args()
log = Path(args.log)
frames = {}
for line in log.read_text().splitlines():
    if line.startswith('MATCH TOOLTIP FRAME: '):
        row = json.loads(line.split(': ', 1)[1])
        frames[row['scene']] = {v['id']: v for v in row['frames']}
fixtures = [ROOT / 'tests/fixtures/match-tooltip-macos-source.json']
if args.include_entry_widths:
    fixtures.append(ROOT / 'tests/fixtures/entry-match-macos-source.json')
checks, files = [], [log, Path(__file__), *fixtures]


def compare(scene, sample, values, threshold, units):
    delta = max(abs(a-b) for a,b in zip(*values)) if all(v is not None for v in values) else None
    checks.append(dict(scene=scene, sample=sample, values=values, maximumDelta=delta,
                       threshold=threshold, units=units, passed=delta is not None and delta <= threshold))


def pixels(im, r):
    return [(x,y,im.getpixel((x,y)))
            for y in range(math.floor(r['y']*2), math.ceil((r['y']+r['h'])*2))
            for x in range(math.floor(r['x']*2), math.ceil((r['x']+r['w'])*2))]


def at(im, r, x, y):
    return im.getpixel((round((r['x']+x)*2), round((r['y']+y)*2)))


def extent(points, r):
    return [min(x for x,y in points)-r['x']*2, min(y for x,y in points)-r['y']*2,
            max(x for x,y in points)-r['x']*2, max(y for x,y in points)-r['y']*2] if points else None


def distance(a, b):
    if not a or not b:
        return None
    def directed(first, second):
        bucket = set(second)
        return max(min((max(abs(dx),abs(dy)) for dy in range(-3,4) for dx in range(-3,4)
                        if (x+dx,y+dy) in bucket), default=4) for x,y in first)
    return max(directed(a,b), directed(b,a))


count = 0
for fixture_path in fixtures:
    fixture = json.loads(fixture_path.read_text())
    for row in fixture['records']:
        count += 1
        scene = row['name']
        native = frames[scene]
        source = {v['id']: v for v in row['styles']}
        paths = [ROOT / fixture['archiveDirectory'] / (scene+'.png'), ROOT / args.native_dir / (scene+'.png')]
        files.extend(paths)
        images = [Image.open(path).convert('RGB') for path in paths]
        assert all(im.size == (row['configuration'].get('windowWidth',1024)*2,1536) for im in images)
        rects = [source['tooltip'], native['tooltip']]
        backgrounds = [at(im,r,2,r['h']/2) for im,r in zip(images,rects)]
        compare(scene, 'tooltip interior fill', backgrounds, 1, 'RGB')
        text_rects = [dict(x=r['x']+14,y=r['y']+8,w=r['w']-28,h=21) for r in rects]
        text_pixels = [[(x,y,c) for x,y,c in pixels(im,r) if max(abs(a-b)for a,b in zip(c,bg))>20]
                       for im,r,bg in zip(images,text_rects,backgrounds)]
        contours = [[(x,y)for x,y,c in group] for group in text_pixels]
        compare(scene,'tooltip text extent',[extent(points,r)for points,r in zip(contours,text_rects)],2,'physical pixels')
        compare(scene,'tooltip text interior foreground',
                [Counter(c for x,y,c in group).most_common(1)[0][0] if group else None for group in text_pixels],1,'RGB')
        delta=distance(*contours)
        checks.append(dict(scene=scene,sample='tooltip internal text footprint',maximumDelta=delta,
                           threshold=2,units='physical pixels',coordinateFrame='absolute PNG pixels',
                           distanceCappedAt=4,passed=delta is not None and delta<=2))
        for vertical in ['top','bottom']:
            for horizontal in ['left','right']:
                corners=[dict(x=r['x']+(0 if horizontal=='left' else r['w']-5),
                              y=r['y']+(0 if vertical=='top' else r['h']-5),w=5,h=5)for r in rects]
                points=[[(x,y)for x,y,c in pixels(im,r)if max(abs(a-b)for a,b in zip(c,bg))<=4]
                        for im,r,bg in zip(images,corners,backgrounds)]
                compare(scene,f'tooltip {vertical}-{horizontal} solid corner extent',
                        [extent(v,r)for v,r in zip(points,corners)],2,'physical pixels')
        # Only the arrow above the body: the lower half is painted over by it.
        arrows=[dict(x=r['x'],y=r['y'],w=r['w'],h=r['h']/2)for r in [source['arrow'],native['arrow']]]
        points=[[(x,y)for x,y,c in pixels(im,r)if max(abs(a-b)for a,b in zip(c,bg))<=4]
                for im,r,bg in zip(images,arrows,backgrounds)]
        compare(scene,'tooltip arrow solid extent',[extent(v,r)for v,r in zip(points,arrows)],2,'physical pixels')
        delta=distance(*points)
        checks.append(dict(scene=scene,sample='tooltip arrow solid footprint',maximumDelta=delta,
                           threshold=2,units='physical pixels',coordinateFrame='absolute PNG pixels',
                           distanceCappedAt=4,passed=delta is not None and delta<=2))
        if args.arrow_shadow and row['configuration']['host']=='entry':
            # In the gap below the input and above the body, outside the solid
            # arrow. Both source and GPUI have the same toolbar background.
            for offset in [-7,7]:
                values=[]
                for im,tip,arrow in zip(images,rects,[source['arrow'],native['arrow']]):
                    x=arrow['x']+arrow['w']/2+offset
                    values.append(im.getpixel((round(x*2),round((tip['y']-3)*2))))
                compare(scene,f'arrow halo gap x{offset:+}',values,1,'RGB')
report=dict(passed=all(c['passed']for c in checks),scenes=count,checks=checks,
            files=[dict(path=str(path.relative_to(ROOT))if path.is_relative_to(ROOT)else str(path),
                        sha256=hashlib.sha256(path.read_bytes()).hexdigest())for path in files],
            arrowHaloChecked=args.arrow_shadow,
            scope='Only tooltip solid fill, foreground, text extent/contour, fixed corner extents and visible solid arrow; optional entry arrow halo uses two fixed gap pixels per state. Unchanged RGB1 and2 physical pixel limits; no complete shadow, anti-alias bitmap, transition, whole image, native or chart acceptance.')
(ROOT/args.output).write_text(json.dumps(report,ensure_ascii=False,indent=2)+'\n')
bad=[c for c in checks if not c['passed']]
for c in bad[:20]:
    print(c)
assert report['passed'],f'{len(bad)} of{len(checks)} bounded tooltip paint checks failed'
print('MATCH TOOLTIP PAINT PASS:',len(checks),'bounded checks across',count,'states; shadow/native/full image separate')
