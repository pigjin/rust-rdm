#!/usr/bin/env python3
"""Bounded tag paint and enabled/disabled search paint, not full image parity."""
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
p.add_argument('--native-dir', default='outputs/ui-qa')
p.add_argument('--output', default='outputs/general-fallback-macos-image-check.json')
args = p.parse_args()
log = Path(args.log)
frames = {}
for line in log.read_text().splitlines():
    if line.startswith('GENERAL FALLBACK FRAME: '):
        row = json.loads(line.split(': ', 1)[1])
        frames[row['scene']] = {m['id']: m for m in row['frames']}
f = json.loads((ROOT / 'tests/fixtures/general-fallback-macos-source.json').read_text())
assert len(frames) == len(f['records']) == 4
checks = []
files = [log, Path(__file__), ROOT / 'tests/fixtures/general-fallback-macos-source.json']


def color_at(image, r, x, y):
    return image.getpixel((round((r['x'] + x) * 2), round((r['y'] + y) * 2)))


def region(image, r):
    return [(x - r['x'] * 2, y - r['y'] * 2, image.getpixel((x, y)))
            for y in range(math.ceil(r['y'] * 2), math.floor((r['y'] + r['h']) * 2))
            for x in range(math.ceil(r['x'] * 2), math.floor((r['x'] + r['w']) * 2))]


def ink(image, r, bg):
    points = [(x, y) for x, y, color in region(image, r)
              if max(abs(a - b) for a, b in zip(color, bg)) > 20]
    if not points:
        return None
    return [min(x for x, y in points), min(y for x, y in points),
            max(x for x, y in points), max(y for x, y in points)]


def footprint(image, r, bg):
    # Compare the actual screenshot grid. Subtracting fractional DOM origins
    # invents fractional pixel distances between integer image pixels. Include
    # every intersecting pixel so a clipped final glyph column is not dropped.
    return [(x,y)
            for y in range(math.floor(r['y']*2),math.ceil((r['y']+r['h'])*2))
            for x in range(math.floor(r['x']*2),math.ceil((r['x']+r['w'])*2))
            if max(abs(a-b) for a,b in zip(image.getpixel((x,y)),bg))>20]


def contour_distance(first, second):
    if not first or not second:
        return None
    def directed(points, other):
        buckets={}
        for q in other:
            buckets.setdefault((math.floor(q[0]),math.floor(q[1])),[]).append(q)
        maximum=0
        for x,y in points:
            candidates=[q for dy in range(-3,4) for dx in range(-3,4)
                        for q in buckets.get((math.floor(x)+dx,math.floor(y)+dy),[])]
            nearest=min((max(abs(x-a),abs(y-b)) for a,b in candidates),default=4)
            maximum=max(maximum,nearest)
        return maximum
    return max(directed(first,second),directed(second,first))


def foreground(image, r, bg):
    pixels = [color for x, y, color in region(image, r)
              if max(abs(a - b) for a, b in zip(color, bg)) > 20]
    return Counter(pixels).most_common(1)[0][0] if pixels else None


def compare(name, sample, values, threshold, units):
    delta = max(abs(a - b) for a, b in zip(*values)) if all(v is not None for v in values) else None
    checks.append({'scene': name, 'sample': sample, 'values': values,
                   'maximumDelta': delta, 'units': units, 'threshold': threshold,
                   'passed': delta is not None and delta <= threshold})


for row in f['records']:
    name = row['name']
    m = {s['id']: s for s in row['styles']}
    a = frames[name]
    paths = [ROOT / f['archiveDirectory'] / (name + '.png'), ROOT / args.native_dir / (name + '.png')]
    files.extend(paths)
    images = [Image.open(path).convert('RGB') for path in paths]
    assert all(image.size == (2048, 1536) for image in images)
    tags = [m['tag'], a['match']]
    backgrounds = [color_at(im, r, 2, 11) for im, r in zip(images, tags)]
    compare(name, 'tag interior fill', backgrounds, 1, 'RGB')
    for edge in ['top', 'bottom', 'left', 'right']:
        colors = []
        for im, r in zip(images, tags):
            x, y = {'top': (r['w'] / 2, .5), 'bottom': (r['w'] / 2, r['h'] - .75),
                    'left': (.5, r['h'] / 2), 'right': (r['w'] - .75, r['h'] / 2)}[edge]
            colors.append(color_at(im, r, x, y))
        compare(name, 'tag ' + edge + ' border interior', colors, 1, 'RGB')
    # Compare the colored border footprint in predetermined 6px corner crops.
    # Antialiased coverage differs between engines; the extent bound remains 2px.
    for vertical in ['top', 'bottom']:
        for horizontal in ['left', 'right']:
            masks = []
            for im, r, bg in zip(images, tags, backgrounds):
                corner = dict(x=r['x'] + (0 if horizontal == 'left' else r['w'] - 6),
                              y=r['y'] + (0 if vertical == 'top' else r['h'] - 6), w=6, h=6)
                masks.append(ink(im, corner, bg))
            compare(name, 'tag ' + vertical + '-' + horizontal + ' border extent', masks, 2, 'physical pixels')
    closes = [m['closeIcon'], dict(x=a['match-close']['x'] - 2, y=a['match-close']['y'] - 2, w=16, h=16)]
    labels = [m['content'], a['match-label']]
    searches = [dict(x=r['x'] + 4, y=r['y'] + 4, w=r['w'] - 8, h=r['h'] - 8)
                for r in [m['search'], a['search']]]
    search_backgrounds = [color_at(im, r, 3, 3) for im, r in zip(images, [m['search'], a['search']])]
    for component, rects, bgs in [('close SVG', closes, backgrounds), ('tag text', labels, backgrounds),
                                 ('search icon', searches, search_backgrounds)]:
        compare(name, component + ' extent', [ink(im, r, bg) for im, r, bg in zip(images, rects, bgs)], 2, 'physical pixels')
        compare(name, component + ' interior foreground', [foreground(im, r, bg) for im, r, bg in zip(images, rects, bgs)], 1, 'RGB')

    selectors = [m['selector'], a['selector']]
    selector_bgs = [color_at(im,r,2,11)for im,r in zip(images,selectors)]
    compare(name,'selector interior fill',selector_bgs,1,'RGB')
    for edge in ['top','bottom','left','right']:
        values=[]
        for im,r in zip(images,selectors):
            x,y={'top':(r['w']/2,.5),'bottom':(r['w']/2,r['h']-.75),'left':(.5,r['h']/2),'right':(r['w']-.75,r['h']/2)}[edge]
            values.append(color_at(im,r,x,y))
        compare(name,'selector '+edge+' border interior',values,1,'RGB')
    glyphs=[dict(x=r['x']+7,y=r['y']+4,w=r['w']-14,h=r['h']-8)for r in selectors]
    compare(name,'selector glyph extent',[ink(im,r,bg)for im,r,bg in zip(images,glyphs,selector_bgs)],2,'physical pixels')
    compare(name,'selector glyph foreground',[foreground(im,r,bg)for im,r,bg in zip(images,glyphs,selector_bgs)],1,'RGB')
    points=[footprint(im,r,bg)for im,r,bg in zip(images,glyphs,selector_bgs)]
    distance=contour_distance(*points)
    checks.append({'scene':name,'sample':'selector glyph internal ink footprint','maximumDelta':distance,'threshold':2,'units':'physical pixels','coordinateFrame':'absolute PNG pixels','passed':distance is not None and distance<=2})

    hints=[dict(x=m['input']['x'],y=m['input']['y']+4,w=min(40,m['input']['w']),h=20),
           dict(x=a['match']['x']+a['match']['w']+4,y=a['outer']['y']+4,
                w=min(40,a['outer']['x']+a['outer']['w']-47-(a['match']['x']+a['match']['w']+4)),h=20)]
    hint_bgs=[color_at(im,r,0,0)for im,r in zip(images,hints)]
    compare(name,'Chinese placeholder glyph extent',[ink(im,r,bg)for im,r,bg in zip(images,hints,hint_bgs)],2,'physical pixels')
    compare(name,'Chinese placeholder foreground',[foreground(im,r,bg)for im,r,bg in zip(images,hints,hint_bgs)],1,'RGB')
    pts=[footprint(im,r,bg)for im,r,bg in zip(images,hints,hint_bgs)];distance=contour_distance(*pts)
    checks.append({'scene':name,'sample':'Chinese placeholder internal ink footprint','maximumDelta':distance,'threshold':2,'units':'physical pixels','coordinateFrame':'absolute PNG pixels','passed':distance is not None and distance<=2})

    contours=[footprint(im,r,bg) for im,r,bg in zip(images,labels,backgrounds)]
    delta=contour_distance(*contours)
    checks.append({'scene':name,'sample':'clipped label internal ink footprint',
                   'inkPixels':[len(points) for points in contours],
                   'maximumDelta':delta,'units':'physical pixels',
                   'coordinateFrame':'absolute PNG pixels','distanceCappedAt':4,
                   'threshold':2,'passed':delta is not None and delta<=2})

native_headers=[]
for row in f['records']:
    im=Image.open(ROOT/args.native_dir/(row['name']+'.png')).convert('RGBA')
    native_headers.append(im.crop((100,80,1500,160)).tobytes())
for index,expected in [(1,True),(2,False),(3,False)]:
    equal=native_headers[0]==native_headers[index]
    checks.append({'scene':'native missing-family reference','sample':'header byte equality against '+f['records'][index]['configuration']['family'], 'actualEqual':equal,'expectedEqual':expected,'passed':equal==expected})

report = {'passed': all(row['passed'] for row in checks), 'scenes': 4, 'checks': checks,
          'logSHA256': hashlib.sha256(log.read_bytes()).hexdigest(),
          'files': [{'path': str(path.relative_to(ROOT)) if path.is_relative_to(ROOT) else str(path),
                     'sha256': hashlib.sha256(path.read_bytes()).hexdigest()} for path in files],
          'scope': 'Only selector/tag fixed fill/border points, predetermined corner extents, tag/close/search foregrounds/extents and label internal ink footprint. A nearest-point distance is capped at4 to report violations beyond unchanged2px; it never accepts a missing region. RGB1 and unchanged2-physical-pixel bounds. Source fixed model may differ outside header. No antialias bitmap, hover/pressed, whole-image or native acceptance.'}
(ROOT / args.output).write_text(json.dumps(report, indent=2) + '\n')
bad = [row for row in checks if not row['passed']]
for row in bad[:20]:
    print(row)
assert report['passed'], f'{len(bad)} matched-tag/search bounded paint checks failed'
print('GENERAL FALLBACK IMAGES PASS:', len(checks), 'bounded checks across4 states; full/native parity separate')
