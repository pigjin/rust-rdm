#!/usr/bin/env python3
"""Check bounded tab colors against recorded source styles, not whole images."""
import hashlib
import json
import math
from pathlib import Path
import re
from PIL import Image

ROOT = Path(__file__).resolve().parents[1]
fixture = json.loads((ROOT / 'tests/fixtures/connection-tabs-source.json').read_text())
mapping = {
    'connection-tabs-light': 'entry-closed-light14',
    'connection-tabs-dark': 'entry-closed-dark14',
    'connection-tabs-general18': 'entry-closed-light18',
    'connection-tabs-dark18': 'entry-closed-dark18',
    'connection-tabs-two-left': 'two-tabs-left-active-dark14',
    'connection-tabs-two-right': 'two-tabs-right-active-dark14',
    'connection-tabs-narrow': 'two-tabs-left-active-narrow-dark14',
    'connection-tabs-mark-light': 'marked-tab-light',
    'connection-tabs-mark-dark': 'marked-tab-dark',
}
states = fixture['layoutStates'] + fixture['programmaticBehaviorStates']
checks = []
files = []


def color(css, background):
    values = [float(number) for number in re.findall(r'[\d.]+', css)]
    assert len(values) in (3, 4), css
    alpha = values[3] if len(values) == 4 else 1
    return tuple(round(values[i] * alpha + background[i] * (1 - alpha)) for i in range(3))


for scene, name in mapping.items():
    state = next(row for row in states if row['name'] == name)
    header = next(row['header'][0] for row in fixture['programmaticBehaviorStates'] if row['theme'] == state['theme'])
    parent = color(header['background'], (0, 0, 0))
    width = state.get('viewportWidth', 1024)
    path = ROOT / f'outputs/ui-qa/{scene}.png'
    image = Image.open(path).convert('RGB')
    assert image.size == (width * 2, 1536)
    files.append({'path': str(path.relative_to(ROOT)), 'sha256': hashlib.sha256(path.read_bytes()).hexdigest()})

    def point(label, x, y, expected):
        pixel = image.getpixel((math.floor(x * 2), math.floor(y * 2)))
        checks.append({'scene': scene, 'sample': label, 'logicalPoint': [x, y], 'expectedRGB': expected,
                       'actualRGB': pixel, 'passed': max(abs(a - b) for a, b in zip(pixel, expected)) <= 1})

    def foreground(label, bounds, expected):
        region = (math.floor(bounds['x'] * 2), math.floor(bounds['y'] * 2),
                  math.ceil((bounds['x'] + bounds['width']) * 2), math.ceil((bounds['y'] + bounds['height']) * 2))
        count = sum(max(abs(a - b) for a, b in zip(pixel, expected)) <= 1
                    for pixel in image.crop(region).get_flattened_data())
        checks.append({'scene': scene, 'sample': label, 'expectedRGB': expected, 'logicalRegion': bounds,
                       'foregroundPixels': count, 'passed': count >= 5})

    point('header fill', width - 5, header['y'] + 8, parent)
    border = color(header['borderBottom'].split('solid ', 1)[1], parent)
    point('header bottom border', width - 5, header['y'] + header['height'] - 0.5, border)
    for index, tab in enumerate(state['tabs']):
        root = tab['root']
        fill = color(root['background'], parent)
        point(f'tab {index} clear right padding', root['x'] + root['width'] - 5, root['y'] + root['height'] / 2, fill)
        top_css = root.get('borderTopColor') or root['borderTop'].split('solid ', 1)[1]
        point(f'tab {index} top border center', root['x'] + root['width'] / 2, root['y'] + 0.5, color(top_css, fill))
        children = tab['children']
        text = next(node for node in children if isinstance(node['class'], str) and 'n-ellipsis' in node['class'].split())
        server = next(node for node in children if node['tag'] == 'I' and isinstance(node['class'], str) and node['class'].startswith('n-icon '))
        close = next(node for node in children if node['class'] == 'n-base-icon')
        for label, node in [('label', text), ('server', server), ('close', close)]:
            foreground(f'tab {index} {label} foreground', node, color(node['color'], fill))

assert len(checks) == 88
report = {
    'passed': all(row['passed'] for row in checks), 'checks': checks, 'files': files,
    'maximumRGBDelta': 1, 'minimumForegroundPixels': 5,
    'scope': '88 bounded candidate fill/border/foreground-presence checks against source CSS, across nine states. Source SVG structures and rectangles are checked separately. No glyph masks, marked corner joins, shadows, hover/pressed states, animation or whole-frame/native equality acceptance.',
}
(ROOT / 'outputs/connection-tabs-color-check.json').write_text(json.dumps(report, indent=2) + '\n')
for row in checks:
    if not row['passed']:
        print(row)
assert report['passed'], 'connection tab bounded color check failed'
print('CONNECTION TABS COLORS PASS: 88 bounded checks in nine states; full-image parity unverified')
