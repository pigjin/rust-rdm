#!/usr/bin/env python3
"""Compare actual GPUI status-info layout and accessible rows to original WK."""
import argparse
import hashlib
import json
from pathlib import Path

ROOT = Path(__file__).resolve().parents[1]
p = argparse.ArgumentParser()
p.add_argument('--log', required=True)
p.add_argument('--output', required=True)
a = p.parse_args()
log = Path(a.log)
fixture_path = ROOT / 'tests/fixtures/status-info-macos-source.json'
fixture = json.loads(fixture_path.read_text())
observed = {}
for line in log.read_text().splitlines():
    if line.startswith('STATUS INFO FRAME: '):
        row = json.loads(line.split(': ', 1)[1])
        observed[row['scene']] = row
checks = []


def equal(scene, measurement, actual, expected):
    checks.append(dict(scene=scene, measurement=measurement, actual=actual,
                       expected=expected, passed=actual == expected))


def bounds(scene, key, source, frames):
    if key not in frames:
        equal(scene, key + '.present', False, True)
        return
    for k in ['x', 'y', 'w', 'h']:
        actual = frames[key][k]
        delta = abs(actual-source[k])
        checks.append(dict(scene=scene, measurement=key+'.'+k, actual=actual,
                           expected=source[k], maximumDelta=delta,
                           threshold=0.6, units='logical pixels', passed=delta < 0.6))


for row in fixture['records']:
    scene = row['name']
    if scene not in observed:
        equal(scene, 'scene.present', False, True)
        continue
    actual = observed[scene]
    frames = {v['id']: v for v in actual['frames']}
    equal(scene, 'selectedGroup', actual['group'], row['filterState']['group'])
    equal(scene, 'keyword', actual['keyword'], row['filterState']['keyword'])
    source_rows = row['rows']
    row_frames = sorted((v for k, v in frames.items() if k.startswith('status-info-row-')),
                        key=lambda v: int(v['id'].rsplit('-', 1)[1]))
    expected_labels = ['\t'.join(c['text'] for c in v['cells']) for v in source_rows]
    equal(scene, 'accessibleRowOrderAndValues', [v['label'] for v in row_frames], expected_labels)
    cards = [v for v in row['layout'] if 'n-card--embedded' in v['class']]
    bounds(scene, 'status-summary-card', cards[0], frames)
    bounds(scene, 'status-activity-card', cards[1], frames)
    bounds(scene, 'status-info-filter', row['inputs'][0]['outer'], frames)
    table = next(v for v in row['layout'] if v['class'] == 'n-data-table-wrapper')
    bounds(scene, 'status-info-table', table, frames)
    groups = [v for v in row['groups'] if v['text'] in row['serverInfo']]
    for group in groups:
        id = 'info-group-' + group['text']
        bounds(scene, id, group, frames)
        equal(scene, id+'.acceptsClick', frames.get(id, {}).get('supportsClick'), not group['disabled'])
    for key, source in zip(['key', 'value'], row['header']):
        bounds(scene, 'status-info-header-' + key, source, frames)
    for i, source in enumerate(source_rows):
        bounds(scene, 'status-info-row-'+str(i), source['outer'], frames)
        for j, cell in enumerate(source['cells']):
            bounds(scene, f'status-info-cell-{i}-{j}', cell, frames)
    if not source_rows:
        equal(scene, 'emptyStateVisible', frames.get('status-info-empty', {}).get('visible'), True)

report = dict(passed=all(v['passed'] for v in checks), scenes=len(fixture['records']),
              checks=checks, logSHA256=hashlib.sha256(log.read_bytes()).hexdigest(),
              fixtureSHA256=hashlib.sha256(fixture_path.read_bytes()).hexdigest(),
              producerSHA256=hashlib.sha256(Path(__file__).read_bytes()).hexdigest(),
              scope='Actual observed INFO table/card/input/group/row/cell bounds and accessible row order/values, plus disabled controls exposing no Click action. Original0.6 logical-pixel bound. Includes offscreen laid-out rows when observed;no whole-image,paint,keyboard/native or clipboard acceptance.')
(ROOT/a.output).write_text(json.dumps(report, ensure_ascii=False, indent=2)+'\n')
bad = [v for v in checks if not v['passed']]
print('STATUS INFO:', len(checks), 'checks;', len(bad), 'differences')
for v in bad[:10]:
    print(v)
raise SystemExit(0 if report['passed'] else 1)
