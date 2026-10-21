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
p.add_argument('--output', default='outputs/tag-style-macos-image-check.json')
args = p.parse_args()
log = Path(args.log)
frames = {}
for line in log.read_text().splitlines():
    if line.startswith('MATCH TAG MACOS FRAME: '):
        row = json.loads(line.split(': ', 1)[1])
        frames[row['scene']] = {m['id']: m for m in row['frames']}
f = json.loads((ROOT / 'tests/fixtures/tag-style-macos-source.json').read_text())
assert len(frames) == len(f['records']) == 6
checks = []
files = [log, Path(__file__), ROOT / 'tests/fixtures/tag-style-macos-source.json']


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
    paths = [ROOT / f['archiveDirectory'] / (name + '.png'), ROOT / 'outputs/ui-qa' / (name + '.png')]
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

report = {'passed': all(row['passed'] for row in checks), 'scenes': 6, 'checks': checks,
          'logSHA256': hashlib.sha256(log.read_bytes()).hexdigest(),
          'files': [{'path': str(path.relative_to(ROOT)) if path.is_relative_to(ROOT) else str(path),
                     'sha256': hashlib.sha256(path.read_bytes()).hexdigest()} for path in files],
          'scope': 'Only fixed tag fill/border points, predetermined corner extents, tag/close/search foregrounds and extents. RGB1 and unchanged2-physical-pixel bounds. Source fixed model may differ outside header. No antialias bitmap, hover/pressed, whole-image or native acceptance.'}
(ROOT / args.output).write_text(json.dumps(report, indent=2) + '\n')
bad = [row for row in checks if not row['passed']]
for row in bad[:20]:
    print(row)
assert report['passed'], f'{len(bad)} matched-tag/search bounded paint checks failed'
print('MATCH TAG MACOS IMAGES PASS:', len(checks), 'bounded checks across6 states; full/native parity separate')
