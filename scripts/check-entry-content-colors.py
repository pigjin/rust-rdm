#!/usr/bin/env python3
"""Check initial and equal-empty display-prop editor frames against Source."""
import json
from pathlib import Path
from PIL import Image

root = Path(__file__).resolve().parents[1]
fixture = json.loads((root / 'tests/fixtures/entry-conversion-edge-source.json').read_text())
panels = json.loads((root / 'tests/fixtures/edit-entry-layout-source.json').read_text())
base = next(record for record in panels['records'] if record['name'] == 'edit-entry-hash-draft')
checks = []
for record in fixture['records']:
    card, field, editor = (record['geometry'][name] for name in ('card', 'field', 'editor'))
    save = base['controls']['save']
    points = {
        'card/action': (card['x'] + card['width'] - 10, card['y'] + card['height'] - 3),
        'field': (field['x'] + field['width'] - 8, field['y'] + 16),
        'editor': (editor['x'] + editor['width'] - 15, editor['y'] + editor['height'] - 15),
        'update fill': (card['x'] + save['x'] - base['controls']['card']['x'] + 5,
                        card['y'] + save['y'] - base['controls']['card']['y'] + 16),
    }
    pictures = [Image.open(root / 'outputs' / folder / (record['name'] + '.png')).convert('RGB')
                for folder in ('ui-reference', 'ui-qa')]
    for name, (x, y) in points.items():
        colors = [picture.getpixel((round(x * 2), round(y * 2))) for picture in pictures]
        delta = max(abs(a - b) for a, b in zip(*colors))
        checks.append({'scene': record['name'], 'sample': name, 'point': [x, y],
                       'source': colors[0], 'native': colors[1], 'maxChannelDelta': delta,
                       'passed': delta <= 1})
    ink = []
    initial = record['name'] == 'entry-conversion-initial'
    bounds = ((field['x'], field['y'] + 5, field['x'] + 55, field['y'] + 27) if initial else
              (editor['x'] + 35, editor['y'], editor['x'] + 180, editor['y'] + 25))
    for picture in pictures:
        crop = picture.crop(tuple(round(value * 2) for value in bounds))
        ink.append(sum(1 for pixel in crop.get_flattened_data()
                       if (max(abs(value - 194) for value in pixel) <= 2 if initial else max(pixel) < 100)))
    checks.append({'scene': record['name'], 'sample': 'initial placeholder ink' if initial else 'retained visible draft ink',
                   'foregroundPixels': ink, 'passed': min(ink) > 10})
report = {'scope': 'Four initial/empty conversion frames; sixteen background/control points at RGB channel tolerance one, one visible placeholder and three retained-draft ink-presence checks. Ink presence does not establish glyph equality. Excludes font/caret/ruler rasterization, animation, other themes/fonts/locales and full-image equality.',
          'checks': checks, 'passed': all(check['passed'] for check in checks)}
(root / 'outputs/entry-content-color-check.json').write_text(json.dumps(report, indent=2) + '\n')
for check in checks:
    if not check['passed']:
        print(check)
assert report['passed'], 'initial/empty conversion color comparison failed'
print('ENTRY CONTENT COLOR PASS:', len(checks), 'original fills and visible-ink checks')
