#!/usr/bin/env python3
"""Compare two pending-conversion editor states with the original frontend."""
import json
from pathlib import Path
from PIL import Image

root = Path(__file__).resolve().parents[1]
fixture = json.loads((root / 'tests/fixtures/entry-conversion-source.json').read_text())
panels = json.loads((root / 'tests/fixtures/edit-entry-layout-source.json').read_text())
base = next(record for record in panels['records'] if record['name'] == 'edit-entry-hash-draft')
checks = []
for record in fixture['records']:
    card, field, editor = (record['geometry'][name] for name in ('card', 'field', 'editor'))
    save = base['controls']['save']
    points = {
        'card/action': (card['x'] + card['width'] - 10, card['y'] + card['height'] - 3),
        'field': (field['x'] + field['width'] - 8, field['y'] + 16),
        'blank editor': (editor['x'] + editor['width'] - 15, editor['y'] + editor['height'] - 15),
        'enabled update': (card['x'] + save['x'] - base['controls']['card']['x'] + 5,
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
report = {'scope': 'Two delayed-conversion layouts and four sampled background/control fills each. RGB channel tolerance one. Excludes glyph/caret/ruler rasterization, animation, other themes/fonts/locales and full-image equality.',
          'checks': checks, 'passed': all(check['passed'] for check in checks)}
(root / 'outputs/entry-conversion-color-check.json').write_text(json.dumps(report, indent=2) + '\n')
for check in checks:
    if not check['passed']:
        print(check)
assert report['passed'], 'pending conversion color comparison failed'
print('ENTRY CONVERSION COLOR PASS:', len(checks), 'original background/control points')
