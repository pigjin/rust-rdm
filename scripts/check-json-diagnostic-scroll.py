#!/usr/bin/env python3
"""Bounded light-theme hover scrolling samples; not complete glyph equality."""
import hashlib
import json
from pathlib import Path
from PIL import Image

ROOT = Path(__file__).resolve().parents[1]
fixture = ROOT / 'tests/fixtures/json-diagnostic-scroll-source.json'
source = json.loads(fixture.read_text())
assert source['sourceCommit'] == '4ddb94a3c55f3401dbc66870f0d21cf0d6203cbc'
assert len(source['records']) == len(source['events']) == 4
assert [e.get('scrollTop') for e in source['events']] == [521, 1041, None, 1041]
assert not source['events'][2]['visible']
checks = []
for record in source['records']:
    name = record['name']
    pictures = [Image.open(ROOT / 'outputs' / folder / (name + '.png')).convert('RGB')
                for folder in ['ui-reference', 'ui-qa']]
    assert all(p.size == (2048, 1536) for p in pictures)
    content = next(m for m in record['metrics'] if m['class'] == 'monaco-hover-content')
    assert record['editorValue'] == '/*' and record['editorScrollTop'] == 0
    assert content['scrollHeight'] == 1291 and content['clientHeight'] == 250
    track = next(m['box'] for m in record['metrics'] if m['class'] == 'visible scrollbar vertical')
    masks = []
    for picture in pictures:
        points = [(x, y) for y in range(round(track['y'] * 2) - 3, round((track['y'] + track['height']) * 2) + 3)
                  for x in range(round(track['x'] * 2) - 3, round((track['x'] + track['width']) * 2) + 3)
                  if any(max(abs(a - b) for a, b in zip(picture.getpixel((x, y)), color)) <= 1 for color in [(186, 186, 186), (179, 179, 179)])]
        masks.append({'pixels': len(points), 'bounds': [min(x for x, y in points), min(y for x, y in points),
                                                       max(x for x, y in points), max(y for x, y in points)] if points else None})
    delta = max(abs(a - b) for a, b in zip(masks[0]['bounds'], masks[1]['bounds'])) if all(m['bounds'] for m in masks) else None
    checks.append({'scene': name, 'sample': 'scrollbar thumb fill and extent', 'expectedRGBOverBodyAndFooter': [[186, 186, 186], [179, 179, 179]],
                   'masks': masks, 'maxExtentDelta': delta, 'passed': delta is not None and delta <= 2})
    point = (round(record['hover']['x'] * 2) + 5, round(record['hover']['y'] * 2) + 20)
    colors = [picture.getpixel(point) for picture in pictures]
    checks.append({'scene': name, 'sample': 'body background', 'point': point, 'actualRGB': colors,
                   'passed': all(c == (243, 243, 243) for c in colors)})
    if name.endswith(('-bottom', '-reopen')):
        point = (round(record['hover']['x'] * 2) + 5, 615)
        colors = [picture.getpixel(point) for picture in pictures]
        checks.append({'scene': name, 'sample': 'scrolled footer background', 'point': point, 'actualRGB': colors,
                       'passed': all(c == (231, 231, 231) for c in colors)})
report = {'scope': 'Four controlled original/native long diagnostic pairs. Bounded light-theme body/footer colors and 10px scrollbar thumb geometry within two physical pixels. No full glyph, Emoji bitmap, antialiasing, native hardware wheel calibration, dark theme, keyboard scrolling or selection auto-scroll equality claimed.',
          'sourceFixtureSha256': hashlib.sha256(fixture.read_bytes()).hexdigest(), 'pairedFrames': 4,
          'checks': checks, 'fullVisualParityComplete': False, 'passed': all(c['passed'] for c in checks)}
(ROOT / 'outputs/json-diagnostic-scroll-color-check.json').write_text(json.dumps(report, indent=2) + '\n')
for check in checks:
    if not check['passed']:
        print(check)
assert report['passed'], 'long diagnostic hover color/extent comparison failed'
print('DIAGNOSTIC HOVER SCROLL PIXEL PASS:', len(checks), 'bounded checks across four pairs')
