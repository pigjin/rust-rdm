#!/usr/bin/env python3
"""Source-backed text lines, actual wheel/pending lifecycle and frozen spinner."""
import argparse, hashlib, json
from collections import Counter
from pathlib import Path
from PIL import Image, ImageChops, ImageFilter

ROOT = Path(__file__).resolve().parents[1]
p = argparse.ArgumentParser()
p.add_argument('--log', required=True)
p.add_argument('--native-dir', required=True)
p.add_argument('--output', required=True)
a = p.parse_args()
frames, completions = {}, {}
for line in Path(a.log).read_text().splitlines():
    if line.startswith('STATUS CLIENTS FRAME: '):
        row = json.loads(line.split(': ', 1)[1]); frames[row['scene']] = row
    elif line.startswith('STATUS CLIENTS PENDING COMPLETION: '):
        row = json.loads(line.split(': ', 1)[1]); completions[row['scene']] = row
paths = [ROOT/'tests/fixtures'/name for name in [
    'status-clients-wrap-lines-macos-source.json',
    'status-clients-loading-phase-macos-source.json',
    'status-clients-extended-v2-macos-source.json',
]]
wrap, loading, extended = [json.loads(path.read_text()) for path in paths]
checks = []

def equal(scene, name, actual, expected):
    checks.append(dict(scene=scene, sample=name, actual=actual, expected=expected, passed=actual == expected))


def near(scene, name, actual, expected, limit=0.6, unit='logical pixels'):
    delta = abs(actual - expected)
    checks.append(dict(scene=scene, sample=name, actual=actual, expected=expected,
                       maximumDelta=delta, threshold=limit, units=unit, passed=delta < limit))


for source in wrap['records']:
    if source['configuration'].get('kind') != 'long': continue
    scene = source['name']; actual = frames[scene]['frames']
    for i, row in enumerate(source['rows']):
        if len(row['cells']) != 5: continue
        for j in range(5):
            expected = [v['text'].rstrip(' ') for v in source['extra']['cells'][6+i*5+j]['lines']]
            lines = sorted((v for v in actual if v['id'].startswith(f'status-clients-line-{i}-{j}-')),
                           key=lambda v:int(v['id'].rsplit('-', 1)[1]))
            equal(scene, f'cell.{i}.{j}.original-grapheme-lines', [v['label'] for v in lines], expected)
            for k, value in enumerate(lines):
                near(scene, f'line.{i}.{j}.{k}.height', value['h'], 21.)
                if k: near(scene, f'line.{i}.{j}.{k}.step', value['y']-lines[k-1]['y'], 21.)

for source in extended['records']:
    if source['configuration'].get('kind') != 'overflow': continue
    scene=source['name']; proof=frames[scene]['scrollProof']
    body=next(v for v in source['layout'] if v['class']=='n-scrollbar-container')
    last=source['rows'][-1]['outer']; expected=last['y']+last['h']-body['y']-body['h']
    equal(scene, 'actual-wheel-to-end-and-back', proof['actualWheelToEndAndBack'], True)
    near(scene, 'source-maximum-scroll-offset', proof['maximumOffset'], expected)
    near(scene, 'last-row-reachable', proof['lastBottom'], proof['bodyBottom'])
    near(scene, 'scroll-top-restored', proof['restoredFirstTop'], proof['initialFirstTop'])

spinner_files=[]
for source in loading['records']:
    scene=source['name']; frame=frames[scene]; proof=frame['pendingProof']; completion=completions[scene]
    for name in ['oldRowsRetained','sortAndRefreshBlocked','actualOwnResponseHeld']:
        equal(scene, name, proof[name], True)
    equal(scene, 'actual-pending-frame', frame['loading'], True)
    equal(scene, 'same-frozen-source-phase', proof['spinnerSeconds'], source['extra']['loadingPhaseSeconds'])
    for name in ['loadingCleared','actualOwnRowsApplied','gateRemoved']:
        equal(scene, name, completion[name], True)
    held=next(v['rows'] for v in frame['serviceLoads'] if v['action']=='pending-refresh')
    equal(scene, 'held-own-response-applied-after-release', completion['rows'], held)
    rect=source['extra']['loadingParts'][0]
    box=(round((rect['x']-4)*2),round((rect['y']-4)*2),round((rect['x']+rect['w']+4)*2),round((rect['y']+rect['h']+4)*2))
    files=[ROOT/loading['archiveDirectory']/(scene+'.png'),ROOT/a.native_dir/(scene+'.png')]
    masks=[];colors=[]
    for path in files:
        crop=Image.open(path).convert('RGB').crop(box);pixels=list(crop.get_flattened_data())
        mask=Image.new('L',crop.size);mask.putdata([255 if r-g>20 and r-b>20 else 0 for r,g,b in pixels]);masks.append(mask)
        selected=[color for color,on in zip(pixels,mask.get_flattened_data()) if on]
        colors.append(Counter(selected).most_common(1)[0][0] if selected else None)
        spinner_files.append(dict(path=str(path.relative_to(ROOT)),sha256=hashlib.sha256(path.read_bytes()).hexdigest()))
    distance=None
    if all(mask.getbbox() for mask in masks):
        for radius in range(5):
            dilated=[mask.filter(ImageFilter.MaxFilter(radius*2+1)) if radius else mask for mask in masks]
            if not ImageChops.subtract(masks[0],dilated[1]).getbbox() and not ImageChops.subtract(masks[1],dilated[0]).getbbox():
                distance=radius;break
    checks.append(dict(scene=scene,sample='spinner.frozen-footprint',maximumDelta=distance,threshold=2,
                       units='physical pixels',pixels=[mask.histogram()[255] for mask in masks],passed=distance is not None and distance<=2))
    delta=max(abs(x-y) for x,y in zip(*colors)) if all(colors) else None
    checks.append(dict(scene=scene,sample='spinner.foreground',values=colors,maximumDelta=delta,threshold=1,
                       units='RGB',passed=delta is not None and delta<=1))

sha=lambda path:hashlib.sha256(path.read_bytes()).hexdigest()
report=dict(passed=all(v['passed'] for v in checks),checks=checks,
            fixtures=[dict(path=str(path.relative_to(ROOT)),sha256=sha(path)) for path in paths],
            spinnerFiles=spinner_files,logSHA256=sha(Path(a.log)),producerSHA256=sha(Path(__file__)),
            scope='3long states compared to original grapheme Range line text;24-row actual wheel to bottom/back;2real-own-response pending/completion lifecycles with source-controlled old rows and actual blocked mouse events;2static CSS+SMIL0.4s spinner footprints. Existing0.6logical/RGB1/2physical limits. No native input,animation timing,scrollbar paint,errors or wholeapp acceptance.')
(ROOT/a.output).write_text(json.dumps(report,ensure_ascii=False,indent=2)+'\n')
bad=[v for v in checks if not v['passed']]
print('CLIENTS EXTENDED:',len(checks),'checks;',len(bad),'differences')
for value in bad[:12]:print(value)
raise SystemExit(0 if report['passed'] else 1)
