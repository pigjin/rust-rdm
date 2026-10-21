#!/usr/bin/env python3
"""Compare bounded footer selection extents across different activation colors.

The original hidden WKWebView uses an inactive gray selection. GPUI's custom
selection uses its active blue. Colors identify each mask, not color parity.
A 3x3 solid neighborhood excludes unselected text antialiasing at gray 210.
"""
import hashlib
import json
import math
from pathlib import Path
from PIL import Image

ROOT = Path(__file__).resolve().parents[1]
fixture = json.loads((ROOT / 'tests/fixtures/json-diagnostic-footer-source.json').read_text())
checks = []
files = [Path(__file__), ROOT / 'tests/fixtures/json-diagnostic-footer-source.json']


def selected_rectangle(image, box, color):
    x, y, width, height = [box[k] for k in ['x', 'y', 'width', 'height']]
    left, top, right, bottom = math.floor(x*2), math.floor(y*2), math.ceil((x+width)*2), math.ceil((y+height)*2)
    def matches(x, y):
        return max(abs(a-b) for a, b in zip(image.getpixel((x, y)), color)) <= 1
    points = {(x, y) for y in range(top+1, bottom-1) for x in range(left+1, right-1)
              if all(matches(x+dx, y+dy) for dx in [-1, 0, 1] for dy in [-1, 0, 1])}
    largest = []
    while points:
        seed = points.pop(); stack = [seed]; component = []
        while stack:
            current = stack.pop(); component.append(current)
            for dx, dy in [(0,1),(0,-1),(1,0),(-1,0)]:
                neighbor = (current[0]+dx, current[1]+dy)
                if neighbor in points:
                    points.remove(neighbor); stack.append(neighbor)
        if len(component) > len(largest): largest = component
    # A selection background has a solid connected area. Isolated gray text
    # edge pixels in an unselected caption must not become a rectangle.
    if len(largest) < 100: return {'solidPixels': len(largest), 'bounds': None}
    return {'solidPixels': len(largest), 'bounds': [min(p[0] for p in largest), min(p[1] for p in largest), max(p[0] for p in largest), max(p[1] for p in largest)]}


for case, source_name, scene in [
    ('character', 'rust-rdm-webkit-footer-tail-view.png', 'entry-diagnostic-keyboard-footer-character'),
    ('boundaries', 'rust-rdm-webkit-footer-view.png', 'entry-diagnostic-keyboard-footer-boundaries'),
    ('word', 'rust-rdm-webkit-footer-word-view.png', 'entry-diagnostic-keyboard-footer-word'),
    ('crossmessage', 'rust-rdm-webkit-footer-multi-view.png', 'entry-diagnostic-keyboard-crossmessage'),
]:
    record = fixture['sequences'][case][-1]
    selected = 'No quick fixes ' if case in ['character', 'boundaries'] else ''
    assert record['selected'].endswith(selected if selected else '\n')
    source = ROOT / 'outputs/source-probes/diagnostic-footer' / source_name
    candidate = ROOT / 'outputs/ui-qa' / (scene + '.png')
    files.extend([source, candidate])
    images = [Image.open(p).convert('RGB') for p in [source, candidate]]
    assert all(im.size == (2048, 1536) for im in images)
    masks = [selected_rectangle(im, record['footerBox'], color) for im, color in zip(images, [(210,210,210), (169,205,245)])]
    bounds = [m['bounds'] for m in masks]
    delta = max(abs(a-b) for a, b in zip(*bounds)) if all(bounds) else None
    passed = (delta is not None and delta <= 2) if selected else bounds == [None, None]
    checks.append({'scene': scene, 'sample': 'footer selection extent' if selected else 'footer start is not painted selected', 'selectedFooterText': selected, 'masks': masks, 'maxPhysicalPixelDelta': delta, 'physicalPixelThreshold': 2, 'passed': passed})
report = {'scope': 'Four bounded original hidden WKWebView / GPUI footer endpoints. Compares selection rectangle extents and absence at footer start only. Original inactive gray and GPUI active blue are separate mask identifiers; no selection-color, glyph, whole-image or actual-native-window parity is claimed.', 'checks': checks, 'files': [{'path': str(p.relative_to(ROOT)), 'sha256': hashlib.sha256(p.read_bytes()).hexdigest()} for p in files], 'fullVisualParityComplete': False, 'passed': all(c['passed'] for c in checks)}
(ROOT / 'outputs/json-diagnostic-footer-color-check.json').write_text(json.dumps(report, ensure_ascii=False, indent=2) + '\n')
for check in checks:
    if not check['passed']: print(check)
assert report['passed'], 'strict footer selection extent comparison failed'
print('FOOTER SELECTION EXTENT PASS: 4 bounded endpoint checks; separate activation colors, unchanged 2 physical pixel threshold')
