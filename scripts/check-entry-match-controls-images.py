#!/usr/bin/env python3
"""Strict matched entry input/button paint diagnostic, separate from tooltips."""
import argparse
import hashlib
import json
from collections import Counter
from pathlib import Path
from PIL import Image

ROOT = Path(__file__).resolve().parents[1]
p = argparse.ArgumentParser()
p.add_argument('--log', required=True)
p.add_argument('--fixture', default='tests/fixtures/entry-match-macos-source.json')
p.add_argument('--native-dir', default='outputs/ui-qa')
p.add_argument('--output', default='outputs/entry-match-controls-image-check.json')
args = p.parse_args()
log = Path(args.log)
frames = {}
for line in log.read_text().splitlines():
    if line.startswith(('MATCH TOOLTIP FRAME: ', 'ENTRY CONTROL FRAME: ')):
        row = json.loads(line.split(': ', 1)[1])
        frames[row['scene']] = {v['id']: v for v in row['frames']}
fixture_path = ROOT / args.fixture
fixture = json.loads(fixture_path.read_text())
checks, files = [], [Path(__file__), log, fixture_path]


def compare(scene, sample, values):
    delta = max(abs(a-b) for a,b in zip(*values)) if all(v is not None for v in values) else None
    checks.append(dict(scene=scene, sample=sample, values=values, maximumDelta=delta,
                       threshold=1, units='RGB', passed=delta is not None and delta<=1))


def color_at(im, r, x, y):
    return im.getpixel((round((r['x']+x)*2),round((r['y']+y)*2)))


for row in fixture['records']:
    scene = row['name']
    source = {v['id']: v for v in row['styles']}
    native = frames[scene]
    paths = [ROOT / fixture['archiveDirectory'] / (scene+'.png'), ROOT / args.native_dir / (scene+'.png')]
    files.extend(paths)
    images = [Image.open(path).convert('RGB') for path in paths]
    for part in ['outer','search']:
        rects = [source[part],native[part]]
        compare(scene,part+' interior fill',[color_at(im,r,3,3)for im,r in zip(images,rects)])
        for edge in ['top','bottom','left','right']:
            values=[]
            for im,r in zip(images,rects):
                x,y={'top':(r['w']/2,.5),'bottom':(r['w']/2,r['h']-.75),
                     'left':(.5,r['h']/2),'right':(r['w']-.75,r['h']/2)}[edge]
                values.append(color_at(im,r,x,y))
            compare(scene,part+' '+edge+' border interior',values)
    foregrounds=[]
    for im,r in zip(images,[source['search'],native['search']]):
        bg=color_at(im,r,3,3)
        colors=[im.getpixel((x,y))for y in range(round((r['y']+4)*2),round((r['y']+r['h']-4)*2))
                for x in range(round((r['x']+4)*2),round((r['x']+r['w']-4)*2))]
        colors=[c for c in colors if max(abs(a-b)for a,b in zip(c,bg))>20]
        foregrounds.append(Counter(colors).most_common(1)[0][0]if colors else None)
    compare(scene,'disabled search text foreground',foregrounds)
report=dict(passed=all(v['passed']for v in checks),scenes=len(fixture['records']),checks=checks,
            files=[dict(path=str(path.relative_to(ROOT))if path.is_relative_to(ROOT)else str(path),
                        sha256=hashlib.sha256(path.read_bytes()).hexdigest())for path in files],
            scope='Original unmodified input/button fill and fixed border/text foreground samples only. Same RGB1 threshold. Separate from the accepted tooltip fill/arrow/text and input geometry checks; failures remain unaccepted. No whole-image/native acceptance.')
(ROOT/args.output).write_text(json.dumps(report,indent=2)+'\n')
bad=[v for v in checks if not v['passed']]
for v in bad[:12]:
    print(v)
print('MATCHED ENTRY CONTROL PAINT:',len(checks)-len(bad),'/',len(checks),'passed;',len(bad),'differences retained')
raise SystemExit(0 if report['passed'] else 1)
