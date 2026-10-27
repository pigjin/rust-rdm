#!/usr/bin/env python3
"""Original client table bounds, actual mouse outcomes and stable row order."""
import argparse, hashlib, json
from pathlib import Path

ROOT = Path(__file__).resolve().parents[1]
p = argparse.ArgumentParser(); p.add_argument('--log', required=True); p.add_argument('--output', required=True); p.add_argument('--fixture', default='tests/fixtures/status-clients-macos-source.json'); a = p.parse_args()
source_path = ROOT/a.fixture
fixture = json.loads(source_path.read_text()); frames = {}
for line in Path(a.log).read_text().splitlines():
    if line.startswith('STATUS CLIENTS FRAME: '):
        row = json.loads(line.split(': ', 1)[1]); frames[row['scene']] = row
checks = []


def equal(scene, name, actual, expected):
    checks.append(dict(scene=scene, sample=name, actual=actual, expected=expected, passed=actual == expected))


def bounds(scene, name, nodes, expected):
    actual = nodes.get(name)
    equal(scene, name+'.present', actual is not None, True)
    if actual is None: return
    for key in ['x', 'y', 'w', 'h']:
        delta = abs(actual[key]-expected[key])
        checks.append(dict(scene=scene, sample=name+'.'+key, actual=actual[key], expected=expected[key], maximumDelta=delta, threshold=0.6, units='logical pixels', passed=delta < 0.6))


for source in fixture['records']:
    scene = source['name']; frame = frames[scene]; nodes = {v['id']:v for v in frame['frames']}
    popup = source['layout'][0]; table = next(v for v in source['layout'] if v['class'] == 'n-data-table-wrapper')
    body = next(v for v in source['layout'] if v['class'] == 'n-scrollbar-container')
    for name, expected in [('status-clients-popup', popup), ('status-clients-table', table), ('clients-scroll', body), ('show-clients', source['trigger']), ('refresh-clients', source['buttons'][0])]:
        bounds(scene, name, nodes, expected)
    sort = None
    for header in source['header']:
        key = header['key']; name = 'status-clients-title' if key == 'title' else 'status-clients-header-'+key
        bounds(scene, name, nodes, header)
        if header.get('sorter'):
            equal(scene, name+'.click', nodes.get(name, {}).get('supportsClick'), True)
            markup = header['sorter']
            order = 'descend' if '--desc' in markup else 'ascend' if '--asc' in markup else None
            if order: sort = dict(field=key, order=order)
    equal(scene, 'sort-state', frame['sort'], sort)
    rows = source['rows']
    # Naive's empty table still has a tbody row containing the empty state.
    data_rows = [v for v in rows if len(v['cells']) == 5]
    expected_labels = ['\t'.join(c['text'] for c in v['cells']) for v in data_rows]
    native_rows = sorted((v for v in frame['frames'] if v['id'].startswith('status-clients-row-')), key=lambda v:int(v['id'].rsplit('-',1)[1]))
    mounted_indices = [int(v['id'].rsplit('-', 1)[1]) for v in native_rows]
    equal(scene, 'displayed-row-order-and-renumbering', [v['label'] for v in native_rows], [expected_labels[i] for i in mounted_indices])
    for i, data in enumerate(data_rows):
        if data['outer']['y'] >= body['y'] + body['h'] or data['outer']['y'] + data['outer']['h'] <= body['y']:
            if i not in mounted_indices: continue
        bounds(scene, f'status-clients-row-{i}', nodes, data['outer'])
        for j, cell in enumerate(data['cells']): bounds(scene, f'status-clients-cell-{i}-{j}', nodes, cell)
    empty = next((v for v in source['layout'] if v['class'].startswith('n-empty ')), None)
    if empty:
        for name, key in [('status-clients-empty', None), ('status-clients-empty-icon', 'n-empty__icon'), ('status-clients-empty-description', 'n-empty__description')]:
            bounds(scene, name, nodes, empty if key is None else next(v for v in source['layout'] if v['class'] == key))
    expected_actions = ['open']
    if source['configuration'].get('refresh'): expected_actions.append('refresh')
    if source['configuration'].get('loading'): expected_actions.append('pending-refresh')
    if source['configuration'].get('reopen'): expected_actions.append('reopen')
    equal(scene, 'actual-service-actions', [v['action'] for v in frame['serviceLoads']], expected_actions)
    equal(scene, 'source-request-count', len(frame['serviceLoads']), len(source['requests']))
    equal(scene, 'own-service-responses-observed', all(v['rows'] for v in frame['serviceLoads']), True)
    if source['configuration'].get('loading'):
        equal(scene, 'pending-load-visible', frame.get('loading', False), True)
        bounds(scene, 'status-clients-loading', nodes, source['extra']['loadingParts'][0])

sha = lambda path:hashlib.sha256(path.read_bytes()).hexdigest()
report = dict(passed=all(v['passed'] for v in checks), scenes=len(fixture['records']), checks=checks, fixtureSHA256=sha(source_path), logSHA256=sha(Path(a.log)), producerSHA256=sha(Path(__file__)), scope=f'{len(fixture["records"])} original client states: identified actual GPUI bounds, stable displayed row order/renumbering, three-state sort and actual own Redis load event counts. Existing0.6logical pixel bound. Controlled four-row visual data installed after real service responses;no native input,paint,alllocales orwholeapp acceptance.')
(ROOT/a.output).write_text(json.dumps(report, ensure_ascii=False, indent=2)+'\n')
bad = [v for v in checks if not v['passed']]; print('CLIENTS LAYOUT:', len(checks), 'checks;', len(bad), 'differences')
for v in bad[:8]: print(v)
raise SystemExit(0 if report['passed'] else 1)
