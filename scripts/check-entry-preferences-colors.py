#!/usr/bin/env python3
"""Check visible parent controls outside the nested modal and its shadow."""
import json
from pathlib import Path
from PIL import Image

root = Path(__file__).resolve().parents[1]
fixture = json.loads((root / 'tests/fixtures/entry-preferences-source.json').read_text())
panels = json.loads((root / 'tests/fixtures/edit-entry-layout-source.json').read_text())
checks = []
for record in fixture['records']:
    scene = record['name']
    base_name = 'edit-entry-hash-full' if (scene.endswith('-full') or scene.endswith('-saved')) else 'edit-entry-hash-draft'
    base = next(item for item in panels['records'] if item['name'] == base_name)
    card, field, editor = record['card'], record['field'], record['value']
    save = base['controls']['save']
    points = {
        'card/action': (card['x'] + card['width'] - 10, card['y'] + card['height'] - 3),
        'field': (field['x'] + field['width'] - 8, field['y'] + 16),
        'editor': (editor['x'] + editor['width'] - 15, editor['y'] + editor['height'] - 15),
        'enabled update': (card['x'] + save['x'] - base['controls']['card']['x'] + 5,
                           card['y'] + save['y'] - base['controls']['card']['y'] + 16),
    }
    pictures = [Image.open(root / 'outputs' / folder / (scene + '.png')).convert('RGB')
                for folder in ('ui-reference', 'ui-qa')]
    for name, (x, y) in points.items():
        colors = [picture.getpixel((round(x * 2), round(y * 2))) for picture in pictures]
        delta = max(abs(a - b) for a, b in zip(*colors))
        checks.append({'scene': scene, 'sample': name, 'point': [x, y], 'source': colors[0],
                       'native': colors[1], 'maxChannelDelta': delta, 'passed': delta <= 1})
# With line numbers and folding disabled, text starts at the editor's left edge.
# The former gutter has no foreground-color ink here, so this checks the visible setting.
record = next(record for record in fixture['records'] if record['name'] == 'entry-preferences-saved')
box = record['value']
ink = {}
for folder in ('ui-reference', 'ui-qa'):
    picture = Image.open(root / 'outputs' / folder / (record['name'] + '.png')).convert('RGB')
    crop = picture.crop(tuple(round(value * 2) for value in (box['x'], box['y'], box['x'] + 30, box['y'] + 40)))
    ink[folder] = sum(1 for pixel in crop.get_flattened_data() if max(abs(value - 212) for value in pixel) <= 1)
checks.append({'scene':record['name'],'sample':'foreground at editor origin without line/folding gutters','foregroundPixels':ink,'passed':min(ink.values())>0})
report = {'scope': 'Five parent-editor layouts, four points each outside the nested modal and its shadow, plus visible foreground at the saved editor origin without line/folding gutters; includes the enabled update fill under the mask. RGB channel tolerance one. Excludes fonts, caret, modal interior/shadow, all themes/fonts/locales and whole-image equality.',
          'checks': checks, 'passed': all(check['passed'] for check in checks)}
(root / 'outputs/entry-preferences-color-check.json').write_text(json.dumps(report, ensure_ascii=False, indent=2) + '\n')
for check in checks:
    if not check['passed']:
        print(check)
assert report['passed'], 'nested preferences parent color comparison failed'
print('ENTRY PREFERENCES COLOR PASS:', len(checks), 'original visible parent points')
