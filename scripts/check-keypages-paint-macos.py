#!/usr/bin/env python3
"""Bounded critical-page paint: stable fills and modal text ink, not whole RGB equality."""
import argparse
import ast
from collections import Counter
import hashlib
import json
import math
from pathlib import Path
from PIL import Image

ROOT = Path(__file__).resolve().parents[1]
parser = argparse.ArgumentParser(description=__doc__)
parser.add_argument('--log', required=True)
parser.add_argument('--native-dir', required=True)
parser.add_argument('--output', required=True)
args = parser.parse_args()
fixture_path = ROOT / 'tests/fixtures/keypages-macos-source.json'
fixture = json.loads(fixture_path.read_text())
frames = {}
for line in Path(args.log).read_text().splitlines():
    if line.startswith('KEYPAGES FRAME: '):
        row = json.loads(line.split(': ', 1)[1])
        nodes = {node['id']: node for node in row['nodes']}
        for node in row['frames']:
            if isinstance(node['id'], str) and node['id'].startswith('Name('):
                nodes.setdefault(json.loads(node['id'][5:-1]), node)
        frames[row['scene']] = nodes
metric_path = ROOT / 'scripts/check-selector-font-macos-images.py'
tree = ast.parse(metric_path.read_text())
definition = next(node for node in tree.body if isinstance(node, ast.FunctionDef) and node.name == 'contour_distance')
namespace = {'math': math}
exec(compile(ast.Module(body=[definition], type_ignores=[]), str(metric_path), 'exec'), namespace)
distance = namespace['contour_distance']
checks = []
files = []


def rgb(scene, id, first, second):
    delta = max(abs(a-b) for a, b in zip(first, second))
    checks.append(dict(scene=scene, element=id, measurement='stable-fill', values=[first, second],
                       maximumDelta=delta, threshold=1, units='RGB', passed=delta <= 1))


def color_at(image, bounds, x, y):
    return image.getpixel((round((bounds['x']+x)*2), round((bounds['y']+y)*2)))


def text_ink(scene, id, source, native, images, inset=0):
    masks, foregrounds = [], []
    for bounds, image in zip([source, native], images):
        box = [math.floor(bounds['x']*2), math.floor(bounds['y']*2),
               math.ceil((bounds['x']+bounds['w'])*2), math.ceil((bounds['y']+bounds['h'])*2)]
        pixels = [(x, y, image.getpixel((x, y))) for y in range(box[1]+inset, box[3]-inset) for x in range(box[0]+inset, box[2]-inset)]
        background = Counter(color for _, _, color in pixels).most_common(1)[0][0]
        ink = [(x, y, color) for x, y, color in pixels if max(abs(a-b) for a, b in zip(color, background)) > 20]
        masks.append([(x, y) for x, y, _ in ink])
        foregrounds.append(Counter(color for _, _, color in ink).most_common(1)[0][0] if ink else None)
    delta = distance(*masks)
    checks.append(dict(scene=scene, element=id, measurement='text-ink-footprint', maximumDelta=delta,
                       threshold=2, units='physical pixels', passed=delta is not None and delta <= 2))
    if all(color is not None for color in foregrounds):
        rgb(scene, id+'.foreground', *foregrounds)


for source in fixture['records']:
    scene = source['name']
    nodes = frames[scene]
    kind = source['configuration']['kind']
    paths = [ROOT / fixture['archiveDirectory'] / (scene+'.png'), ROOT / args.native_dir / (scene+'.png')]
    images = [Image.open(path).convert('RGB') for path in paths]
    files.extend(dict(path=str(path.relative_to(ROOT)), sha256=hashlib.sha256(path.read_bytes()).hexdigest()) for path in paths)
    page = kind.startswith(('slow-', 'monitor-', 'pubsub-', 'log-'))
    root = 'keypages-root' if page else 'key-delete-warning' if kind == 'warning' else 'dialog-card'
    rgb(scene, root, color_at(images[0], source['root'], 6, 6), color_at(images[1], nodes[root], 6, 6))
    if not page:
        if kind != 'warning':
            heading = next(node for node in source['metrics'] if node['class'].startswith('n-dialog__title'))
            text_ink(scene, 'dialog-title', heading, nodes['dialog-title'], images)
        for id, button in zip(['dialog-cancel', 'dialog-save'], source['buttons'][-2:]):
            rgb(scene, id, color_at(images[0], button, button['w']/2, 2), color_at(images[1], nodes[id], nodes[id]['w']/2, 2))
            text_ink(scene, id, button, nodes[id], images, inset=5)
        if kind.startswith('ttl'):
            for id, button in zip(['ttl-quick--1-1', 'ttl-quick-10-1', 'ttl-quick-1-60', 'ttl-quick-1-3600', 'ttl-quick-1-86400'], source['buttons'][:5]):
                rgb(scene, id, color_at(images[0], button, button['w']/2, 2), color_at(images[1], nodes[id], nodes[id]['w']/2, 2))
                text_ink(scene, id, button, nodes[id], images, inset=5)
    else:
        if kind.startswith('monitor-'):
            ids = ['stream-start', 'stream-copy', 'stream-export', 'stream-last', 'stream-clear']
        elif kind.startswith('pubsub-'):
            ids = ['stream-start', 'stream-last', 'stream-clear']
            rgb(scene, 'publish', color_at(images[0], source['buttons'][-1], source['buttons'][-1]['w']/2, 2), color_at(images[1], nodes['publish'], nodes['publish']['w']/2, 2))
        elif kind.startswith('log-'):
            ids = ['log-refresh', 'clear-logs']
        else:
            ids = []
        for id, button in zip(ids, source['buttons']):
            rgb(scene, id, color_at(images[0], button, button['w']/2, 2), color_at(images[1], nodes[id], nodes[id]['w']/2, 2))
            text_ink(scene, id, button, nodes[id], images, inset=5)
        if kind.startswith('pubsub-'):
            text_ink(scene, 'publish', source['buttons'][-1], nodes['publish'], images, inset=5)
        # Fixed table cells are checked independently, including their actual ink,
        # so correct outer bounds cannot hide a different font or column width.
        prefix = kind.split('-')[0]
        headers = [node for node in source['metrics'] if node['tag'] == 'TH']
        for index, node in enumerate(headers):
            id = f'{prefix}-header-{index}'
            aliases = {'slow-header-0':'slow-sort-time', 'slow-header-1':'slow-client-header', 'log-header-0':'log-sort-time'}
            actual = nodes.get(aliases.get(id, id))
            if actual is None:
                checks.append(dict(scene=scene, element=id, measurement='painted-node-present', passed=False))
                continue
            rgb(scene, id+'.fill', color_at(images[0], node, 5, 5), color_at(images[1], actual, 5, 5))
            text_ink(scene, id, node, actual, images, inset=5)
        cells = [node for node in source['metrics'] if node['tag'] == 'TD']
        for index, node in enumerate(cells):
            row, column = divmod(index, 4)
            id = f'{prefix}-cell-{row}-{column}'
            actual = nodes.get(id)
            if actual is None:
                checks.append(dict(scene=scene, element=id, measurement='painted-node-present', passed=False))
                continue
            rgb(scene, id+'.fill', color_at(images[0], node, 5, 5), color_at(images[1], actual, 5, 5))
            if column != 3 or prefix != 'pubsub':
                text_ink(scene, id, node, actual, images, inset=5)
        if prefix == 'monitor':
            node = next(node for node in source['metrics'] if node['class'].startswith('list-wrapper '))
            rgb(scene, 'monitor-code.fill', color_at(images[0], node, 6, 6), color_at(images[1], nodes['keypages-table'], 6, 6))
            for index, node in enumerate(node for node in source['metrics'] if node['class'].startswith('line-item ')):
                text_ink(scene, f'monitor-row-{index}', node, nodes[f'monitor-row-{index}'], images)
        if prefix == 'log':
            node = next(node for node in source['metrics'] if node['tag'] == 'H3')
            text_ink(scene, 'keypages-title', node, nodes['keypages-title'], images)
        if headers:
            table = nodes['keypages-table']
            rgb(scene, 'table-unused-area.fill', color_at(images[0], table, 6, table['h']-6), color_at(images[1], table, 6, table['h']-6))
    # Input and selection surfaces are fixed, visible areas of these controlled
    # states; coordinates come from the original DOM, with current geometry
    # verified separately. The left 6px of a selection can overlap the adjacent
    # focused input shadow. Use a clear interior at 20,6, excluding that shadow,
    # glyphs, corners and borders; RGB1 remains unchanged.
    for index, node in enumerate(node for node in source['metrics'] if node['class'].startswith('n-input ') or node['class'].startswith('n-base-selection ')):
        rgb(scene, f'input-{index}.fill', color_at(images[0], node, 20, 6), color_at(images[1], node, 20, 6))

report = dict(passed=all(row['passed'] for row in checks), sourceStates=len(fixture['records']), checks=checks, files=files,
              sourceFixtureSHA256=hashlib.sha256(fixture_path.read_bytes()).hexdigest(),
              logSHA256=hashlib.sha256(Path(args.log).read_bytes()).hexdigest(),
              producerSHA256=hashlib.sha256(Path(__file__).read_bytes()).hexdigest(),
              contourMetricSHA256=hashlib.sha256(metric_path.read_bytes()).hexdigest(),
              scope='40 actual source/current page images: fixed page, table, input and button fills; table-cell, heading and control ink and foreground. Existing RGB1 and 2 physical pixel contour bounds; not complete antialias, shadow, animation or whole-image equivalence.')
output = ROOT / args.output
output.parent.mkdir(parents=True, exist_ok=True)
output.write_text(json.dumps(report, ensure_ascii=False, indent=2)+'\n')
bad = [row for row in checks if not row['passed']]
print(f'KEYPAGES PAINT: {len(checks)} checks; {len(bad)} differences')
for row in bad[:12]:
    print(row)
raise SystemExit(1 if bad else 0)
