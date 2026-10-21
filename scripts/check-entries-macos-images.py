#!/usr/bin/env python3
"""Bounded collection fills/ink extents, not complete-image/native parity."""
import argparse
import hashlib
import json
import math
from pathlib import Path
from PIL import Image

ROOT = Path(__file__).resolve().parents[1]
parser = argparse.ArgumentParser()
parser.add_argument('--log', required=True)
args = parser.parse_args()
log = Path(args.log)
frames = {}
models = {}
legacy = []
for line in log.read_text().splitlines():
    if line.startswith('ENTRIES MACOS FRAME: '):
        row = json.loads(line.split(': ', 1)[1])
        frames[row['scene']] = {m['id']: m for m in row['frames']}
        models[row['scene']] = row['rows']
    elif line.startswith('ENTRIES RETAINED LEGACY COMPARISON: '):
        legacy.append(json.loads(line.split(': ', 1)[1]))
fixture = json.loads((ROOT / 'tests/fixtures/entries-macos-source.json').read_text())
assert len(frames) == len(fixture['records']) == 24 and len(legacy) == 14
checks = []
files = [log, Path(__file__), ROOT / 'tests/fixtures/entries-macos-source.json']


def has(metric, name):
    return name in metric['class'].split()


def element(name, index=None):
    return f'Name("{name}")' if index is None else f'NamedInteger("{name}", {index})'


def point(image, rect, x, y):
    return image.getpixel((round((rect['x'] + x) * 2), round((rect['y'] + y) * 2)))


def ink(image, rect, background):
    left, top = math.ceil((rect['x'] + 2) * 2), math.ceil((rect['y'] + 2) * 2)
    right, bottom = math.floor((rect['x'] + rect['w'] - 2) * 2), math.floor((rect['y'] + rect['h'] - 2) * 2)
    pixels = [(x, y) for y in range(top, bottom) for x in range(left, right)
              if max(abs(a-b) for a, b in zip(image.getpixel((x, y)), background)) > 40]
    if not pixels:
        return None
    # Relative to the component, so window rounding is measured separately by
    # the strict component rectangles rather than added twice to this mask.
    ox, oy = rect['x'] * 2, rect['y'] * 2
    return [min(x for x, _ in pixels)-ox, min(y for _, y in pixels)-oy,
            max(x for x, _ in pixels)-ox, max(y for _, y in pixels)-oy]


for row in fixture['records']:
    name = row['name']
    source_path = ROOT / fixture['archiveDirectory'] / (name + '.png')
    actual_path = ROOT / 'outputs/ui-qa' / (name + '.png')
    files.extend([source_path, actual_path])
    source, actual = [Image.open(p).convert('RGB') for p in (source_path, actual_path)]
    assert source.size == actual.size == (row['width'] * 2, 1536)
    metrics = row['metrics']
    headers = [m for m in metrics if has(m, 'n-data-table-th')]
    mapped = []
    for header in headers:
        key = {'#': 'number', '字段': 'field', 'ID': 'field', '值': 'value', '分值': 'score', '操作': 'actions'}[header['text']]
        mapped.append(('header-' + key, header, frames[name][element('heading-entry-' + key)]))
    columns = len(headers)
    for index, cell in enumerate(m for m in metrics if has(m, 'n-data-table-td')):
        key = {'#': 'number', '字段': 'field', 'ID': 'field', '值': 'value', '分值': 'score', '操作': 'action-cell'}[headers[index % columns]['text']]
        mapped.append((f'row{index//columns}-' + key, cell, frames[name][element('entry-' + key, index//columns)]))
    for label, reference, candidate in mapped:
        source_bg = point(source, reference, 3, 3)
        actual_bg = point(actual, candidate, 3, 3)
        color_delta = max(abs(a-b) for a, b in zip(source_bg, actual_bg))
        checks.append({'scene': name, 'component': label, 'sample': 'interior fill',
                       'sourceRGB': source_bg, 'actualRGB': actual_bg, 'maximumRGBDelta': color_delta,
                       'passed': color_delta <= 1})
        # Set scan order comes directly from Redis and is not sorted by either
        # implementation. Fills stay positional; value ink uses the same member.
        if row['kind'] == 'SET' and label.startswith('row') and label.endswith('-value'):
            index = next(i for i, value in enumerate(models[name]) if value['value'] == reference['text'])
            candidate = frames[name][element('entry-value', index)]
            actual_bg = point(actual, candidate, 3, 3)
        masks = [ink(image, rect, bg) for image, rect, bg in
                 [(source, reference, source_bg), (actual, candidate, actual_bg)]]
        extent_delta = max(abs(a-b) for a, b in zip(*masks)) if all(masks) else None
        checks.append({'scene': name, 'component': label, 'sample': 'visible ink extent',
                       'masks': masks, 'maximumPhysicalPixelDelta': extent_delta,
                       'physicalPixelThreshold': 2,
                       'passed': extent_delta is not None and extent_delta <= 2})
report = {'passed': all(c['passed'] for c in checks), 'scenes': 24, 'checks': checks,
          'legacyChromeComparisons': legacy, 'legacyChromeAllPassed': all(c['passed'] for c in legacy),
          'setInkMatchedByActualMember': True,
          'logSHA256': hashlib.sha256(log.read_bytes()).hexdigest(),
          'files': [{'path': str(p.relative_to(ROOT)) if p.is_relative_to(ROOT) else str(p),
                     'sha256': hashlib.sha256(p.read_bytes()).hexdigest()} for p in files],
          'scope': 'Only interior fills and relative visible-ink rectangles in table headers/body cells. Set fills stay positional, value ink is matched by actual member because Redis scan order differs from fixed source props. Contrast40 selects ink, not text-color parity. Unchanged2-physical-pixel extent threshold. No full glyph bitmap, input caret/border/focus, animation, whole image or actual native acceptance.'}
(ROOT / 'outputs/entries-macos-image-check.json').write_text(json.dumps(report, ensure_ascii=False, indent=2) + '\n')
bad = [row for row in checks if not row['passed']]
for row in bad[:16]:
    print(row)
assert report['passed'], f'{len(bad)} bounded collection image checks failed'
print(f'MACOS COLLECTION IMAGES PASS:{len(checks)} bounded fills/ink checks in24 states; legacy failures retained; full/native parity unverified')
