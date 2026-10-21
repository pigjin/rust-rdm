#!/usr/bin/env python3
"""Compare diagnostic colors and extents in the app's own rendered scenes.

SVG shape provenance is checked separately. These bounded masks deliberately
do not assert equality of antialiasing, complete glyphs, or entire images.
"""
import json
from pathlib import Path
from PIL import Image

ROOT = Path(__file__).resolve().parents[1]
hover = json.loads((ROOT / 'tests/fixtures/json-diagnostic-hover-source.json').read_text())
extra = json.loads((ROOT / 'tests/fixtures/json-diagnostic-render-source.json').read_text())
checks = []
known_differences = []


def images(name):
    result = [Image.open(ROOT / 'outputs' / folder / (name + '.png')).convert('RGB')
              for folder in ['ui-reference', 'ui-qa']]
    assert result[0].size == result[1].size == (2048, 1536)
    return result


def mask(picture, bounds, color):
    x1, y1, x2, y2 = bounds
    points = [(x, y) for y in range(y1, y2) for x in range(x1, x2)
              if max(abs(a - b) for a, b in zip(picture.getpixel((x, y)), color)) <= 1]
    return {'pixels': len(points), 'bounds': ([min(p[i] for p in points) for i in [0, 1]]
                                           + [max(p[i] for p in points) for i in [0, 1]]) if points else None}


scenes = [{'name': r['editorFrame'], 'dark': r['dark'], 'editor': r['editor'],
           'decorations': [dict(d['bounds'], **{'class': 'squiggly-error'}) for d in r['squiggle']]}
          for r in hover['records']] + extra['records']
for record in scenes:
    name = record['name']
    pictures = images(name)
    dark = record.get('dark', False)
    editor = record['editor']
    warning = 'warning' in name
    wave = (191, 136, 3) if warning else ((241, 76, 76) if dark else (229, 20, 0))
    ruler = (191, 136, 3) if warning else ((188, 22, 22) if dark else (255, 89, 89))
    for index, decoration in enumerate(record['decorations']):
        # Source's CSS rounds decoration widths to whole logical pixels.
        x, y, w, h = (decoration[k] for k in ['x', 'y', 'width', 'height'])
        bounds = tuple(round(v * 2) for v in [x - 2, y + h - 4, x + w + 2, y + h + 1])
        masks = [mask(picture, bounds, wave) for picture in pictures]
        delta = max(abs(a - b) for a, b in zip(masks[0]['bounds'], masks[1]['bounds'])) if all(m['bounds'] for m in masks) else None
        extent_passed = delta is not None and delta <= 2
        checks.append({'scene': name, 'sample': 'underline', 'decoration': index,
                       'expectedRGB': wave, 'masks': masks, 'extentMaxPixelDelta': delta,
                       'extentPassed': extent_passed,
                       'passed': extent_passed})
    bounds = tuple(round(v * 2) for v in [editor['x'] + editor['width'] - 5,
                                        editor['y'] + 2, editor['x'] + editor['width'],
                                        editor['y'] + editor['height']])
    masks = [mask(picture, bounds, ruler) for picture in pictures]
    delta = max(abs(a - b) for a, b in zip(masks[0]['bounds'], masks[1]['bounds'])) if all(m['bounds'] for m in masks) else None
    checks.append({'scene': name, 'sample': 'overview ruler', 'expectedRGB': ruler,
                   'masks': masks, 'extentMaxPixelDelta': delta,
                   'passed': delta is not None and delta <= 2})
    if name.startswith('entry-diagnostic-editor'):
        foreground = (212, 212, 212) if dark else (0, 0, 0)
        bounds = tuple(round(v * 2) for v in [editor['x'] + 33, editor['y'],
                                            editor['x'] + 160, editor['y'] + 17])
        masks = [mask(picture, bounds, foreground) for picture in pictures]
        checks.append({'scene': name, 'sample': 'editor foreground', 'expectedRGB': foreground,
                       'masks': masks, 'passed': all(m['pixels'] > 100 for m in masks)})

panels = json.loads((ROOT / 'tests/fixtures/edit-entry-layout-source.json').read_text())
for record in panels['records']:
    name = record['name']
    pictures = images(name)
    c = record['controls']['save']
    bounds = tuple(round(v * 2) for v in [c['x'] + 28, c['y'] + 8, c['x'] + 58, c['y'] + 24])
    dark = 'dark' in name
    disabled = name in ['edit-entry-hash-dark-default', 'edit-entry-hash-dark-field-focus']
    enabled = name in ['edit-entry-hash-draft', 'edit-entry-hash-full',
                       'edit-entry-hash-full-dark', 'edit-entry-hash-dark']
    if enabled:
        foreground = (211, 58, 49)
    elif dark:
        foreground = (15, 15, 17) if disabled else (0, 0, 0)
    else:
        foreground = (255, 255, 255)
    masks = [mask(picture, bounds, foreground) for picture in pictures]
    checks.append({'scene': name, 'sample': 'update text foreground', 'expectedRGB': foreground,
                   'masks': masks, 'passed': all(m['pixels'] > 100 for m in masks)})

report = {'scope': 'Six diagnostic frames: sampled fully covered error/warning colors, six underline extent frames including Emoji (two physical pixels tolerance), six ruler extents, two editor foreground checks and thirteen Update foreground checks. SVG polygon provenance is checked separately. Does not assert antialiasing, whole glyphs, ruler border/cursor rasterization, hover, schema validation, IME or full-image equality.',
          'knownDifferences': known_differences, 'fullVisualParityComplete': False,
          'checks': checks, 'passed': all(check['passed'] for check in checks)}
(ROOT / 'outputs/json-diagnostics-color-check.json').write_text(json.dumps(report, indent=2) + '\n')
for check in checks:
    if not check['passed']:
        print(check)
assert report['passed'], 'JSON diagnostic extent or foreground comparison failed'
print('JSON DIAGNOSTIC COLOR PASS:', len(checks), 'bounded masks/colors; full-image equality not asserted')
