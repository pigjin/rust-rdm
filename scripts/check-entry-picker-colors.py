#!/usr/bin/env python3
"""Compare original dropdown surfaces and option/control backgrounds."""
import json
from pathlib import Path
from PIL import Image

root = Path(__file__).resolve().parents[1]
fixture = json.loads((root / 'tests/fixtures/entry-picker-source.json').read_text())
checks = []

def check(scene, label, x, y):
    pictures = [Image.open(root / 'outputs' / folder / (scene + '.png')).convert('RGB')
                for folder in ('ui-reference', 'ui-qa')]
    colors = [picture.getpixel((round(x * 2), round(y * 2))) for picture in pictures]
    delta = max(abs(a - b) for a, b in zip(*colors))
    checks.append({'scene': scene, 'sample': label, 'point': [x, y], 'source': colors[0],
                   'native': colors[1], 'maxChannelDelta': delta, 'passed': delta <= 1})

for record in fixture['records']:
    scene = record['name']
    controls = record['controls']
    box = controls['trigger']
    # Popup shadows cross the trigger; they are outside this fill-color check.
    if not controls['menu'] and not controls['tooltip']:
        check(scene, 'unobscured trigger', box['x'] + 3, box['y'] + 27)
    if controls['menu']:
        box = controls['menu']
        check(scene, 'surface', box['x'] + 7, box['y'] + 4)
        for option in record['options']:
            box = option['bounds']
            check(scene, 'option ' + option['label'], box['x'] + 8, box['y'] + 14)
    if controls['tooltip']:
        box = controls['tooltip']
        check(scene, 'tooltip', box['x'] + 6, box['y'] + 5)

# The same quaternary-button hover policy is used by the existing value footer.
footer = json.loads((root / 'tests/fixtures/value-footer-extra-source.json').read_text())
record = next(record for record in footer['records'] if record['name'] == 'footer-format-tooltip-dark')
box = next(box for box in record['metrics'] if 'n-button ' in box['class'] and box['text'] == 'Raw')
check(record['name'], 'shared dark hover', box['x'] + 3, box['y'] + 27)
check(record['name'], 'footer background', 1018, 751)
report = {'scope': 'Twelve entry picker layouts: trigger, popup, option and tooltip background points; one existing footer dark-hover point. Trigger pixels under popup/tooltip shadows are excluded. RGB channel tolerance one. Excludes glyphs, shadows, borders, caret, full-image equality and animation timing.',
          'checks': checks, 'passed': all(check['passed'] for check in checks)}
(root / 'outputs/entry-picker-color-check.json').write_text(json.dumps(report, ensure_ascii=False, indent=2) + '\n')
for result in checks:
    if not result['passed']:
        print(result)
assert report['passed'], 'entry picker color comparison failed'
print('ENTRY PICKER COLOR PASS:', len(checks), 'original background points')
