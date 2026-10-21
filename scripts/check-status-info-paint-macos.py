#!/usr/bin/env python3
"""Bounded INFO table/group paint; original RGB1 and2physical-pixel limits."""
import argparse, ast, hashlib, json, math
from collections import Counter
from pathlib import Path
from PIL import Image, ImageChops

ROOT = Path(__file__).resolve().parents[1]
p = argparse.ArgumentParser(); p.add_argument('--native-dir', required=True); p.add_argument('--log', required=True); p.add_argument('--output', required=True); a = p.parse_args()
fixture_path = ROOT/'tests/fixtures/status-info-macos-source.json'; fixture = json.loads(fixture_path.read_text())
metric = ROOT/'scripts/check-selector-font-macos-images.py'
definition = next(v for v in ast.parse(metric.read_text()).body if isinstance(v, ast.FunctionDef) and v.name == 'contour_distance')
namespace = {'math': math}; exec(compile(ast.Module(body=[definition], type_ignores=[]), str(metric), 'exec'), namespace); distance = namespace['contour_distance']
checks = []; exact = []; files = []; sha = lambda p: hashlib.sha256(p.read_bytes()).hexdigest()


def sample(scene, name, images, x, y):
    at = (round(x*2), round(y*2)); values = [im.getpixel(at) for im in images]
    delta = max(abs(x-y) for x, y in zip(*values))
    checks.append(dict(scene=scene, sample=name, physicalPoint=at, values=values, maximumDelta=delta, threshold=1, units='RGB', passed=delta <= 1))


def text(scene, name, images, box):
    crops = [im.crop(box) for im in images]; masks = []; foregrounds = []
    for im in crops:
        bg = im.getpixel((0, 0)); differences = ImageChops.difference(im, Image.new('RGB', im.size, bg)).split()
        mask = ImageChops.lighter(ImageChops.lighter(differences[0], differences[1]), differences[2]).point(lambda v: 255 if v > 20 else 0)
        masks.append([(i % im.width, i // im.width) for i, v in enumerate(mask.get_flattened_data()) if v])
        values = [v for v, ink in zip(im.get_flattened_data(), mask.get_flattened_data()) if ink]
        foregrounds.append(Counter(values).most_common(1)[0][0] if values else None)
    delta = distance(*masks) if any(masks) else 0
    checks.append(dict(scene=scene, sample=name+'.footprint', pixels=[len(v) for v in masks], maximumDelta=delta, threshold=2, units='physical pixels', passed=delta is not None and delta <= 2))
    delta = max(abs(x-y) for x, y in zip(*foregrounds)) if all(v is not None for v in foregrounds) else None
    checks.append(dict(scene=scene, sample=name+'.foreground', values=foregrounds, maximumDelta=delta, threshold=1, units='RGB', passed=delta is not None and delta <= 1))


for row in fixture['records']:
    scene = row['name']; paths = [ROOT/fixture['archiveDirectory']/(scene+'.png'), ROOT/a.native_dir/(scene+'.png')]
    images = [Image.open(v).convert('RGB') for v in paths]; files.extend(dict(path=str(v.relative_to(ROOT)), sha256=sha(v)) for v in paths)
    table = next(v for v in row['layout'] if v['class'] == 'n-data-table-wrapper')
    for group in [v for v in row['groups'] if v['text'] in row['serverInfo']]:
        sample(scene, 'group.'+group['text']+'.fill', images, group['x']+4, group['y']+14)
    for i, cell in enumerate(row['header']):
        sample(scene, f'header.{i}.fill', images, cell['x']+4, cell['y']+4)
        box = tuple(round(v*2) for v in [cell['x']+10, cell['y']+6, cell['x']+cell['w']-10, cell['y']+cell['h']-6])
        text(scene, f'header.{i}.text', images, box)
    for i, data in enumerate(row['rows']):
        # Both renderers lay out all rows. Pixels below the scroll viewport
        # are clipped; do not label those row coordinates as painted evidence.
        if data['outer']['y']+data['outer']['h'] > table['y']+table['h']-1:
            continue
        for j, cell in enumerate(data['cells']):
            sample(scene, f'row.{i}.{j}.fill', images, cell['x']+4, cell['y']+4)
            box = tuple(round(v*2) for v in [cell['x']+10, cell['y']+6, cell['x']+cell['w']-10, cell['y']+cell['h']-6])
            text(scene, f'row.{i}.{j}.text', images, box)
    card = next(v for v in row['layout'] if 'n-card--embedded' in v['class'] and 'flex-item-expand' in v['class'])
    box = tuple(round(v*2) for v in [card['x'], card['y'], card['x']+card['w'], card['y']+card['h']])
    delta = ImageChops.difference(*[im.crop(box) for im in images]); bad = sum(any(v) for v in delta.get_flattened_data())
    exact.append(dict(scene=scene, changedRGBPixels=bad, maximumChannelDelta=max(v[1] for v in delta.getextrema()), threshold=0, passed=bad == 0))
report = dict(passed=all(v['passed'] for v in checks), checks=checks, exactCardCropPassed=all(v['passed'] for v in exact), exactCardCrops=exact, files=files,
    sourceFixtureSHA256=sha(fixture_path), logSHA256=sha(Path(a.log)), producerSHA256=sha(Path(__file__)), contourMetricSHA256=sha(metric),
    scope='Actual6 INFO PNGs:fixed group/header/visible-cell fill samples and text ink footprints/foreground. Same RGB1 and2physical-pixel contour limits. Exact card crops independently retained even when bounded checks pass. No empty-state/icon/fullpage/native interaction acceptance.')
(ROOT/a.output).write_text(json.dumps(report, ensure_ascii=False, indent=2)+'\n'); bad = [v for v in checks if not v['passed']]
print('STATUS INFO PAINT', len(checks), 'checks;', len(bad), 'differences;exact crops', sum(v['passed'] for v in exact), '/6')
for v in bad[:12]: print(v)
raise SystemExit(0 if report['passed'] else 1)
