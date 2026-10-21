#!/usr/bin/env python3
"""Compare bounded modal colors; do not accept full frames, glyphs or animation."""
import hashlib
import json
import math
from pathlib import Path
import re
from PIL import Image

ROOT = Path(__file__).resolve().parents[1]
fixture = json.loads((ROOT / 'tests/fixtures/tab-close-source.json').read_text())
checks = []
files = []


def color(css, background):
    values = [float(value) for value in re.findall(r'[\d.]+', css)]
    assert len(values) in (3, 4), css
    alpha = values[3] if len(values) == 4 else 1
    return tuple(round(values[i] * alpha + background[i] * (1 - alpha)) for i in range(3))


for layout in fixture['layouts']:
    theme = layout['theme']
    parts = layout['parts']
    scene = 'tab-close-warning' + ('-dark' if theme == 'dark' else '')
    paths = [ROOT / f'outputs/source-probes/tab-close/{theme}-frame.png', ROOT / f'outputs/ui-qa/{scene}.png']
    images = [Image.open(path).convert('RGB') for path in paths]
    assert all(image.size == (2048, 1536) for image in images)
    files += [{'path': str(path.relative_to(ROOT)), 'sha256': hashlib.sha256(path.read_bytes()).hexdigest()} for path in paths]
    background = color(parts['card']['background'], (0, 0, 0))

    def point(label, x, y, expected):
        observed = [image.getpixel((math.floor(x * 2), math.floor(y * 2))) for image in images]
        passed = all(max(abs(a - b) for a, b in zip(pixel, expected)) <= 1 for pixel in observed)
        checks.append({'scene': scene, 'sample': label, 'logicalPoint': [x, y], 'expectedRGB': expected, 'observedRGB': observed, 'passed': passed})

    def foreground(label, bounds, expected):
        region = (math.floor(bounds['x'] * 2), math.floor(bounds['y'] * 2), math.ceil((bounds['x'] + bounds['width']) * 2), math.ceil((bounds['y'] + bounds['height']) * 2))
        counts = []
        for image in images:
            pixels = image.crop(region)
            counts.append(sum(max(abs(a - b) for a, b in zip(pixel, expected)) <= 1 for pixel in pixels.get_flattened_data()))
        checks.append({'scene': scene, 'sample': label, 'logicalRegion': bounds, 'expectedRGB': expected, 'sourceAndCandidateForegroundPixels': counts, 'passed': min(counts) >= 5})

    card = parts['card']
    point('card upper blank fill', card['x'] + card['width'] - 8, card['y'] + 8, background)
    point('card lower blank fill', card['x'] + 8, card['y'] + card['height'] - 8, background)
    title = dict(parts['title'])
    title['x'] += 32
    title['width'] -= 32
    foreground('title foreground', title, color(parts['title']['color'], background))
    foreground('content foreground', parts['content'], color(parts['content']['color'], background))
    icon = dict(parts['title'])
    icon['width'] = 28
    foreground('warning icon foreground', icon, color(parts['confirm']['background'], background))
    confirm = parts['confirm']
    fill = color(confirm['background'], background)
    point('confirm fill', confirm['x'] + 5, confirm['y'] + 5, fill)
    foreground('confirm caption', confirm, color(confirm['color'], fill))
    cancel = parts['cancel']
    point('cancel transparent body', cancel['x'] + 5, cancel['y'] + 5, background)
    raw = json.loads((ROOT / f'outputs/source-probes/tab-close/{theme}-raw.json').read_text())
    children = raw['records'][0]['state']['states'][-1]['dialogs'][0]['children']
    source_border = next(node['border'] for node in children if node['class'] == 'n-button__border')
    border = color(source_border.split('solid ', 1)[1], background)
    foreground('cancel border foreground', cancel, border)

report = {
    'passed': all(row['passed'] for row in checks), 'checks': checks, 'files': files,
    'maximumRGBDelta': 1, 'minimumForegroundPixels': 5,
    'scope': '18 bounded modal fill/foreground presence checks in two images. Geometry is checked separately by actual GPUI events. No glyph masks, antialiasing, hover/pressed colors, shadows, animation or whole-image parity acceptance.',
}
(ROOT / 'outputs/tab-close-color-check.json').write_text(json.dumps(report, indent=2) + '\n')
for row in checks:
    if not row['passed']:
        print(row)
assert report['passed'], 'tab close bounded color check failed'
print('TAB CLOSE COLORS PASS: 18 bounded checks across two themes; full-image parity unverified')
