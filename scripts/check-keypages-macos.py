#!/usr/bin/env python3
"""Compare controlled current GPUI page/dialog bounds with original WKWebView."""
import argparse
import hashlib
import json
import re
from pathlib import Path

ROOT = Path(__file__).resolve().parents[1]
parser = argparse.ArgumentParser(description=__doc__)
parser.add_argument('--log', required=True)
parser.add_argument('--output', required=True)
args = parser.parse_args()
fixture_path = ROOT / 'tests/fixtures/keypages-macos-source.json'
fixture = json.loads(fixture_path.read_text())
frames = {}
for line in Path(args.log).read_text().splitlines():
    if line.startswith('KEYPAGES FRAME: '):
        row = json.loads(line.split(': ', 1)[1])
        frames[row['scene']] = row
checks = []


def geometry(scene, id, actual, expected):
    if actual is None or expected is None:
        checks.append({'scene': scene, 'element': id, 'measurement': 'painted-node-present', 'passed': False})
        return
    for dimension in ['x', 'y', 'w', 'h']:
        delta = abs(actual[dimension] - expected[dimension])
        checks.append({'scene': scene, 'element': id, 'measurement': dimension, 'actual': actual[dimension],
                       'expected': expected[dimension], 'maximumDelta': delta,
                       'threshold': .6, 'units': 'logical pixels', 'passed': delta < .6})


def first(source, prefix):
    return next((node for node in source['metrics'] if node['class'].startswith(prefix)), None)


def same(scene, element, actual, expected):
    checks.append({'scene': scene, 'element': element, 'measurement': 'rendered-label',
                   'actual': actual, 'expected': expected, 'passed': actual == expected})


for source in fixture['records']:
    scene = source['name']
    frame = frames.get(scene)
    if frame is None:
        checks.append({'scene': scene, 'measurement': 'actual-scene-captured', 'passed': False})
        continue
    nodes = {node['id']: node for node in frame['nodes']}
    for node in frame['frames']:
        if isinstance(node['id'], str) and node['id'].startswith('Name('):
            nodes.setdefault(json.loads(node['id'][5:-1]), node)
    kind = source['configuration']['kind']
    page = kind.startswith(('slow-', 'monitor-', 'pubsub-', 'log-'))
    root = 'keypages-root' if page else 'key-delete-warning' if kind == 'warning' else 'dialog-card'
    geometry(scene, root, nodes.get(root), source['root'])
    if page:
        geometry(scene, 'keypages-form', nodes.get('keypages-form'), first(source, 'n-form '))
        if not kind.startswith('monitor-'):
            geometry(scene, 'keypages-table', nodes.get('keypages-table'), first(source, 'n-data-table '))
        if kind.startswith('monitor-'):
            ids = ['stream-start', 'stream-copy', 'stream-export', 'stream-last', 'stream-clear']
        elif kind.startswith('pubsub-'):
            ids = ['stream-start', 'stream-last', 'stream-clear']
            geometry(scene, 'publish', nodes.get('publish'), source['buttons'][-1])
        elif kind.startswith('log-'):
            ids = ['log-refresh', 'clear-logs']
            geometry(scene, 'log-server-filter', nodes.get('log-server-filter'), first(source, 'n-base-selection '))
        else:
            ids = []
            geometry(scene, 'slow-refresh', nodes.get('slow-refresh'), source['buttons'][-1])
            for id, button in zip(['slow-limit-step--1', 'slow-limit-step-1'], source['buttons'][:2]):
                geometry(scene, id, nodes.get(id), button)
        for id, button in zip(ids, source['buttons']):
            geometry(scene, id, nodes.get(id), button)
        prefix = kind.split('-')[0]
        headers = [node for node in source['metrics'] if node['tag'] == 'TH']
        aliases = {'slow-header-0':'slow-sort-time', 'log-header-0':'log-sort-time'}
        for column, node in enumerate(headers):
            id = f'{prefix}-header-{column}'
            geometry(scene, id, nodes.get(aliases.get(id, id)), node)
        if prefix == 'log':
            geometry(scene, 'keypages-title', nodes.get('keypages-title'), next(node for node in source['metrics'] if node['tag'] == 'H3'))
        rows = sorted((node for node in frame['frames'] if f'{prefix}-row-' in str(node['id'])),
                      key=lambda node: int(re.search(r'-row-(\d+)', node['id']).group(1)))
        if prefix == 'monitor':
            expected_rows = [node for node in source['metrics'] if node['class'].startswith('line-item ')]
            same(scene, 'displayed-row-count', len(rows), len(expected_rows))
            for index, (actual, expected) in enumerate(zip(rows, expected_rows)):
                geometry(scene, f'{prefix}-row-{index}', actual, expected)
                same(scene, f'{prefix}-row-{index}', actual['label'], expected['text'].removeprefix('>').strip())
        else:
            data = [row for row in source['rows'] if len(row) == 4]
            same(scene, 'displayed-row-count', len(rows), len(data))
            cells = [node for node in source['metrics'] if node['class'].startswith('n-data-table-td')]
            for index, (actual, expected) in enumerate(zip(rows, data)):
                same(scene, f'{prefix}-row-{index}', actual['label'], '\t'.join(expected[:3] if prefix == 'pubsub' else expected))
                group = cells[index * 4:index * 4 + 4]
                if len(group) == 4:
                    for column, node in enumerate(group):
                        id = f'{prefix}-cell-{index}-{column}'
                        geometry(scene, id, nodes.get(id), node)
                    bounds = dict(x=min(node['x'] for node in group), y=min(node['y'] for node in group),
                                  w=max(node['x']+node['w'] for node in group)-min(node['x'] for node in group),
                                  h=max(node['y']+node['h'] for node in group)-min(node['y'] for node in group))
                    geometry(scene, f'{prefix}-row-{index}', actual, bounds)
    else:
        title = 'key-delete-warning-title' if kind == 'warning' else 'dialog-title'
        body = 'key-delete-warning-content' if kind == 'warning' else 'dialog-body'
        geometry(scene, title, nodes.get(title), first(source, 'n-dialog__title'))
        geometry(scene, body, nodes.get(body), first(source, 'n-dialog__content'))
        if kind != 'warning':
            same(scene, 'dialog-title', nodes.get(title, {}).get('label'), first(source, 'n-dialog__title')['text'])
        for id, button in zip(['dialog-cancel', 'dialog-save'], source['buttons'][-2:]):
            geometry(scene, id, nodes.get(id), button)
        if kind.startswith('ttl'):
            geometry(scene, 'ttl-unit', nodes.get('ttl-unit'), first(source, 'n-base-selection '))
            for id, button in zip(['ttl-quick--1-1', 'ttl-quick-10-1', 'ttl-quick-1-60', 'ttl-quick-1-3600', 'ttl-quick-1-86400'], source['buttons'][:5]):
                geometry(scene, id, nodes.get(id), button)

report = {'passed': all(row['passed'] for row in checks), 'sceneCount': len(frames), 'sourceStates': len(fixture['records']),
          'checks': checks, 'sourceFixtureSHA256': hashlib.sha256(fixture_path.read_bytes()).hexdigest(),
          'captureLogSHA256': hashlib.sha256(Path(args.log).read_bytes()).hexdigest(),
          'producerSHA256': hashlib.sha256(Path(__file__).read_bytes()).hexdigest(),
          'scope': 'Actual GPUI paint-observed roots, tables, forms, headings, content and buttons against 40 original WK states. Existing 0.6 logical pixel bound. Does not assert whole-image or unobserved behavior.'}
output = ROOT / args.output
output.parent.mkdir(parents=True, exist_ok=True)
output.write_text(json.dumps(report, ensure_ascii=False, indent=2) + '\n')
bad = [row for row in checks if not row['passed']]
print(f'KEYPAGES LAYOUT: {len(frames)} states; {len(checks)} checks; {len(bad)} differences')
for row in bad[:12]:
    print(row)
raise SystemExit(1 if bad else 0)
