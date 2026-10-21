#!/usr/bin/env python3
"""Build current offline review with raw tooltip and entry control crops."""
import base64
import hashlib
import json
import math
import re
import runpy
from pathlib import Path

from PIL import Image

ROOT = Path(__file__).resolve().parents[1]
OUT = ROOT / 'outputs'
audit = json.loads((OUT / 'entry-controls-full-execution-audit.json').read_text())
assert audit['requiredSceneExecutionPassed'] and audit['fullScenes'] == 477
for script in ['make-key-filter-crops.py', 'make-tag-style-crops.py', 'make-tag-overflow-crops.py',
               'make-sidebar-macos-crops.py', 'make-selector-font-crops.py', 'make-css-label-crops.py',
               'make-parity-review.py']:
    runpy.run_path(str(ROOT / 'scripts' / script), run_name='__main__')
page = OUT / 'parity-review.html'
html = page.read_text()
matched = re.search(r'const scenes=(.*?);const pages=', html, re.S)
assert matched
scenes = json.loads(matched.group(1))
previous_count = len(scenes)
assert previous_count == 426
records = []
for filename in ['match-tooltip-macos-source.json', 'entry-match-macos-source.json', 'entry-controls-macos-source.json']:
    fixture = json.loads((ROOT / 'tests/fixtures' / filename).read_text())
    for row in fixture['records']:
        styles = {v['id']: v for v in row['styles']}
        targets = [styles[k] for k in ['tag', 'tooltip', 'arrow'] if k in styles]
        if row['configuration']['host'] == 'entry':
            targets.append(styles['group'])
        width = row['configuration'].get('windowWidth', 1024)
        box = (max(0, math.floor(min(v['x'] for v in targets)*2)-24),
               max(0, math.floor(min(v['y'] for v in targets)*2)-24),
               min(width*2, math.ceil(max(v['x']+v['w'] for v in targets)*2)+24),
               min(1536, math.ceil(max(v['y']+v['h'] for v in targets)*2)+24))
        record = {'scene': row['name'], 'physicalPixelCrop': box}
        pair = {'name': 'macOS · ' + ('集合输入 · ' if filename == 'entry-controls-macos-source.json' else '匹配提示 · ') + row['name']}
        for kind, source, folder in [
            ('native', OUT / 'ui-qa' / (row['name']+'.png'), OUT / 'ui-qa-match-tooltip-crops'),
            ('reference', ROOT / fixture['archiveDirectory'] / (row['name']+'.png'), OUT / 'ui-reference-match-tooltip-crops'),
        ]:
            folder.mkdir(exist_ok=True)
            target = folder / (row['name']+'.png')
            with Image.open(source) as image:
                assert image.size == (width*2, 1536)
                image.crop(box).save(target)
            record[kind] = dict(source=str(source.relative_to(ROOT)), sourceSHA256=hashlib.sha256(source.read_bytes()).hexdigest(),
                                path=str(target.relative_to(ROOT)), sha256=hashlib.sha256(target.read_bytes()).hexdigest())
            pair[kind] = 'data:image/png;base64,' + base64.b64encode(target.read_bytes()).decode()
        records.append(record)
        scenes.append(pair)
assert len(records) == 24 and len(scenes) == 450
html = html[:matched.start(1)] + json.dumps(scenes, ensure_ascii=False) + html[matched.end(1):]
html = html.replace('此页包含 426 组', '此页包含 450 组', 1)
description = '<p>新增 24 对原始像素裁片：20 组匹配提示和集合输入宽度，另有 4 组无提示框的输入与搜索按钮。颜色、阴影采样与尺寸分别记录；过渡动画和整图一致性仍待验收。</p>\n'
html = html.replace('<div class="controls">', description+'<div class="controls">', 1)
page.write_text(html)
(OUT / 'entry-controls-crops.json').write_text(json.dumps(dict(passed=True, scenes=24, records=records,
    scope='Unmodified two-times PNG pixels, source-defined control/tooltip union plus12 logical pixels, clipped to actual viewport. Shadow may extend outside crop. No shadow, transition, whole-image or native equality acceptance.'), indent=2)+'\n')
(OUT / 'entry-controls-review-build.json').write_text(json.dumps(dict(passed=True, previousPairs=previous_count,
    newPairs=24, totalPairs=len(scenes), htmlSHA256=hashlib.sha256(page.read_bytes()).hexdigest(),
    producerSHA256=hashlib.sha256(Path(__file__).read_bytes()).hexdigest(),
    browserInteractionVerified=False, scope='Offline page generation only; no browser opened or prior safety restriction bypassed.'), indent=2)+'\n')
print('PREPARED450 PAIRS: previous426 retained,24 raw tooltip/control crops appended; browser acceptance separate')
