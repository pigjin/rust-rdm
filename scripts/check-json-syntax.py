#!/usr/bin/env python3
"""Check bounded foreground colors in real entry and scalar JSON screenshots."""
import argparse
import hashlib
import json
import math
from pathlib import Path
from PIL import Image

ROOT = Path(__file__).resolve().parents[1]
parser = argparse.ArgumentParser(description=__doc__)
parser.add_argument('--log', type=Path, default=Path('/private/tmp/rust-rdm-json-syntax-full-ui.log'))
args = parser.parse_args()
log = args.log.read_text()
frames = [json.loads(line.split('QA SYNTAX FRAME: ', 1)[1]) for line in log.splitlines() if line.startswith('QA SYNTAX FRAME: ')]
names = ['entry-diagnostic-syntax-light', 'entry-diagnostic-syntax-dark', 'json-syntax-light', 'json-syntax-dark']
assert [row['scene'] for row in frames] == names
source = json.loads((ROOT / 'tests/fixtures/json-syntax-source.json').read_text())

def rgb(css):
    return tuple(int(value.strip()) for value in css.split('(', 1)[1].rstrip(')').split(','))

def color_of(text, dark, fragment):
    sample = next(row for row in source['samples'] if row['text'] == text and row['dark'] == dark)
    return rgb(next(span['color'] for line in sample['lines'] for span in line['spans'] if span['text'] == fragment))

checks = []
for row in frames:
    path = ROOT / 'outputs/ui-qa' / (row['scene'] + '.png')
    image = Image.open(path).convert('RGB')
    assert image.size == (2048, 1536)
    scale = image.width / 1024
    dark = row['dark']
    comment = color_of('/*', dark, '/*')
    x, y, height = row['firstCodeX'], row['firstLineY'], row['lineHeight']
    right = row['input'][0] + row['input'][2]
    regions = [
        ('comment', (x, y, right, y + height), comment),
        ('brace-inside-comment', (row['commentBraceX'], y, row['commentBraceX'] + row['commentBraceWidth'], y + height), comment),
        ('property', (x, y + height, right, y + 2 * height), color_of('{"a":true,"b":false}', dark, '"a"')),
        ('keyword', (x, y + height, right, y + 2 * height), color_of('true', dark, 'true')),
        ('punctuation', (x, y + height, right, y + 2 * height), color_of('{"a":null}', dark, ':')),
        ('balanced-brace', (x, y + height, right, y + 2 * height), color_of('{"a":null}', dark, '{')),
    ]
    for role, bounds, expected in regions:
        bounds = (math.floor(bounds[0] * scale), math.floor(bounds[1] * scale), math.ceil(bounds[2] * scale), math.ceil(bounds[3] * scale))
        pixels = image.crop(bounds)
        count = sum(max(abs(actual - target) for actual, target in zip(pixel, expected)) <= 1 for pixel in pixels.getdata())
        checks.append({'scene': row['scene'], 'role': role, 'expectedRGB': expected, 'physicalBounds': bounds,
                       'matchingForegroundPixels': count, 'minimumMatchingPixels': 5, 'maximumRGBChannelDelta': 1,
                       'passed': count >= 5, 'imageSHA256': hashlib.sha256(path.read_bytes()).hexdigest()})
report = {
    'scope': '24 bounded foreground-presence checks in four real GPUI editor images. Regions come from fresh editor/caret geometry; source colors come from original Monaco DOM spans. Does not compare entire glyph masks, fonts, antialiasing, focus outlines or whole frames.',
    'checks': checks, 'passed': all(row['passed'] for row in checks), 'fullVisualParityComplete': False,
    'logSHA256': hashlib.sha256(args.log.read_bytes()).hexdigest(),
}
(ROOT / 'outputs/json-syntax-color-check.json').write_text(json.dumps(report, indent=2) + '\n')
assert report['passed'], [row for row in checks if not row['passed']]
print('JSON SYNTAX COLORS PASS: 24 bounded foreground checks across four entry/scalar images')
