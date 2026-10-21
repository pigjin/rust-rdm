#!/usr/bin/env python3
"""One Menlo 14px Emoji crop versus an isolated WKWebView font probe.

This is not a Wails page, native-window interaction or general glyph equality.
"""
import hashlib
import json
from pathlib import Path
from PIL import Image, ImageChops

ROOT = Path(__file__).resolve().parents[1]
OUT = ROOT / 'outputs'
log = Path('/private/tmp/rust-rdm-macos-font-scroll-full-ui.log').read_text()
assert 'MACOS FONT QA PASS: 231 ' in log
assert 'outputs/ui-qa/entry-diagnostic-unicode.png\n' in log
metadata = json.loads((OUT / 'font-bitmap-webkit-source.json').read_text())
assert metadata['width'] == 19 and metadata['pixelRatio'] == 2
assert metadata['viewport'] == [400, 100]
paths = [OUT / 'font-reference-webkit/menlo-14-emoji.png',
         OUT / 'ui-qa/entry-diagnostic-unicode.png']
images = [Image.open(path).convert('RGB') for path in paths]
assert [image.size for image in images] == [(800, 200), (2048, 1536)]

def yellow_face(image, region):
    x1, y1, x2, y2 = region
    # Exclude neutral text and red diagnostic antialiasing (whose G == B).
    points = []
    for y in range(y1, y2):
        for x in range(x1, x2):
            r, g, b = image.getpixel((x, y))
            if r > 150 and g > 90 and b < 150 and g - b > 40:
                points.append((x, y))
    assert points
    bounds = [min(x for x, y in points), min(y for x, y in points),
              max(x for x, y in points), max(y for x, y in points)]
    return {'pixels': len(points), 'bounds': bounds,
            'extent': [bounds[2] - bounds[0] + 1, bounds[3] - bounds[1] + 1]}

masks = [yellow_face(images[0], (0, 0, 800, 200)),
         yellow_face(images[1], (1418, 642, 1710, 715))]
assert all(mask['extent'] == [31, 31] for mask in masks)
assert masks[0]['pixels'] == masks[1]['pixels'] == 696
crops = []
for image, mask in zip(images, masks):
    x1, y1, x2, y2 = mask['bounds']
    crops.append(image.crop((x1 - 2, y1 - 2, x2 + 3, y2 + 3)))
assert all(crop.size == (35, 35) for crop in crops)
differences = list(ImageChops.difference(*crops).get_flattened_data())
max_delta = max(max(pixel) for pixel in differences)
report = {
    'scope': 'One Menlo 14px Emoji at DPR 2; independent generated WKWebView font page versus current GPUI diagnostic editor. Yellow-face extents and a padded 35x35 crop only. Does not verify other glyphs/sizes/themes, full text rasterization, original Wails page geometry, native-window interaction or complete-image parity.',
    'referenceEngine': 'macOS WKWebView', 'fontSize': 14, 'devicePixelRatio': 2,
    'masks': masks, 'paddedCropSize': [35, 35],
    'maxRGBChannelDelta': max_delta, 'maxAllowedRGBChannelDelta': 1,
    'differentCropPixels': sum(any(pixel) for pixel in differences),
    'files': [{'path': str(path.relative_to(ROOT)),
               'sha256': hashlib.sha256(path.read_bytes()).hexdigest()} for path in paths],
    'sourceMetadataSha256': hashlib.sha256((OUT / 'font-bitmap-webkit-source.json').read_bytes()).hexdigest(),
    'probeSourceSha256': hashlib.sha256((OUT / 'source-probes/font-bitmap-webkit.swift').read_bytes()).hexdigest(),
    'fullVisualParityComplete': False, 'passed': max_delta <= 1,
}
(OUT / 'font-bitmap-webkit-check.json').write_text(json.dumps(report, indent=2) + '\n')
assert report['passed'], report
print('WKWEBVIEW FONT BITMAP PASS: one 35x35 Emoji crop, same 31x31 face extent, maximum RGB delta', max_delta)
