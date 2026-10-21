#!/usr/bin/env python3
"""Check native chart layout/splines against captured original Chart.js state.

This compares production renderer inputs/geometry, not raster equivalence.
"""
import argparse
import hashlib
import json
from pathlib import Path

ROOT = Path(__file__).resolve().parents[1]
p = argparse.ArgumentParser()
p.add_argument('--log', required=True)
p.add_argument('--fixture', default='tests/fixtures/status-layout-macos-source.json')
p.add_argument('--output', required=True)
a = p.parse_args()
log = Path(a.log)
fixture_path = ROOT / a.fixture
fixture = json.loads(fixture_path.read_text())
observed = {}
active = None
for line in log.read_text().splitlines():
    if line.startswith('STATUS LAYOUT START: '):
        active = line.split(': ', 1)[1]
    elif active and line.startswith('STATUS CHART FRAME: '):
        row = json.loads(line.split(': ', 1)[1])
        observed.setdefault(active, {})[row['chart']] = row
    elif line.startswith('STATUS LAYOUT FRAME: '):
        active = None
checks = []


def numeric(scene, chart, name, actual, expected, threshold=0.6):
    delta = abs(actual-expected)
    checks.append(dict(scene=scene, chart=chart, measurement=name,
                       actual=actual, expected=expected, maximumDelta=delta,
                       threshold=threshold, passed=delta < threshold))


for row in fixture['records']:
    name = row['name']
    for color, original in zip(['ff6384', 'ff9f40', '9966ff', '4bc0c0'], row['charts']):
        actual = observed[name][color]
        layout = actual['layout']
        for k in ['x', 'y', 'w', 'h']:
            numeric(name, color, 'canvas.'+k, actual['canvas'][k], original['canvas'][k])
        for k in ['left', 'top', 'right', 'bottom']:
            numeric(name, color, 'chartArea.'+k, layout[k], original['chartArea'][k])
        for k in ['min', 'max']:
            numeric(name, color, 'y.'+k, layout[k], original['scales']['y'][k], 1e-6)
        numeric(name, color, 'x.rotation', layout['rotation'],
                original['scales']['x']['labelRotation'] * 3.141592653589793/180, 1e-6)
        checks.append(dict(scene=name, chart=color, measurement='x.visibleTickIndices',
                           actual=layout['x_ticks'], expected=[t['value'] for t in original['scales']['x']['ticks']],
                           passed=layout['x_ticks'] == [t['value'] for t in original['scales']['x']['ticks']]))
        checks.append(dict(scene=name, chart=color, measurement='y.ticksAndLabels',
                           actual=list(zip(layout['ticks'], layout['tick_labels'])),
                           expected=[(t['value'], t['label']) for t in original['scales']['y']['ticks']],
                           passed=list(zip(layout['ticks'], layout['tick_labels'])) == [(t['value'], t['label']) for t in original['scales']['y']['ticks']]))
        for i, box in enumerate(original['legend']['hitBoxes']):
            for actual_key, expected_key in [('x', 'left'), ('y', 'top'), ('w', 'width'), ('h', 'height')]:
                numeric(name, color, f'legend.{i}.{actual_key}', layout['legends'][i][actual_key], box[expected_key])
        for i, dataset in enumerate(original['datasets']):
            for j, point in enumerate(dataset['points']):
                native = layout['points'][i][j]
                checks.append(dict(scene=name, chart=color, measurement=f'point.{i}.{j}.skip', actual=native['skip'], expected=point['skip'], passed=native['skip'] == point['skip']))
                for k in ['x', 'y']:
                    numeric(name, color, f'point.{i}.{j}.{k}', native[k], point[k])
                # Skipped points do not contribute a painted cubic segment.
                if not point['skip']:
                    for side, key in [('previous', 'cp1'), ('next', 'cp2')]:
                        for k in ['x', 'y']:
                            numeric(name, color, f'point.{i}.{j}.{side}.{k}', native[side][k], point[key+k])

report = dict(passed=all(c['passed'] for c in checks), checks=checks,
              scenes=len(fixture['records']), charts=sum(len(v['charts']) for v in fixture['records']),
              logSHA256=hashlib.sha256(log.read_bytes()).hexdigest(),
              sourceFixtureSHA256=hashlib.sha256(fixture_path.read_bytes()).hexdigest(),
              producerSHA256=hashlib.sha256(Path(__file__).read_bytes()).hexdigest(),
              scope='Actual production native chart geometry, visible ticks, legends, finite point/control-point coordinates and NaN skips; unchanged 0.6 logical pixel limit and 1e-6 domains/rotation. Does not accept text rasterization, clipping, fill/stroke pixels, native lifecycle or whole UI.')
(ROOT/a.output).write_text(json.dumps(report, ensure_ascii=False, indent=2)+'\n')
bad = [c for c in checks if not c['passed']]
print('STATUS CHARTS:', len(checks), 'checks;', len(bad), 'differences')
for c in bad[:12]:
    print(c)
raise SystemExit(0 if report['passed'] else 1)
