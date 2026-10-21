#!/usr/bin/env python3
"""Verify the independent original status-value fixture, not product behavior."""
import hashlib
import json
from pathlib import Path
import subprocess

ROOT = Path(__file__).resolve().parents[1]
fixture = json.loads((ROOT / 'tests/fixtures/status-values-source.json').read_text())
for row in fixture['files']:
    assert hashlib.sha256((ROOT / row['path']).read_bytes()).hexdigest() == row['sha256'], row['path']
raw = ROOT / fixture['rawReport']
assert hashlib.sha256(raw.read_bytes()).hexdigest() == fixture['rawSHA256']
assert json.loads(raw.read_text()) == fixture['records']
archive = raw.parent
for filename, original in [('ContentServerStatus.vue', 'frontend/src/components/content_value/ContentServerStatus.vue'), ('byte_convert.js', 'frontend/src/utils/byte_convert.js')]:
    data = subprocess.check_output(['git', '-C', '/Users/zlsj/Projects/tiny-rdm', 'show', fixture['sourceCommit'] + ':' + original])
    assert data == (archive / filename).read_bytes() == (Path('/Users/zlsj/Projects/tiny-rdm') / original).read_bytes()
done = subprocess.run(['/Users/zlsj/.nvm/versions/node/v26.4.0/bin/node', str(archive / 'source-evaluate.mjs')], capture_output=True, text=True)
assert done.returncode == 0, done.stderr
assert json.loads(done.stdout) == fixture['records']
counts = {key: len(value) for key, value in fixture['records'].items()}
assert counts == {'uptime': 14, 'memory': 18, 'chartBytes': 14, 'integerParsing': 21, 'samples': 6}
report = {'passed': True, 'counts': counts, 'sourceCommit': fixture['sourceCommit'], 'rawSHA256': fixture['rawSHA256'], 'scope': 'Original pure component/functions/provenance and reproducible Node evaluation only; product unit and actual GPUI checks separate, no native/chart/whole-image acceptance.'}
(ROOT / 'outputs/status-values-source-check.json').write_text(json.dumps(report, indent=2) + '\n')
print('ORIGINAL STATUS VALUE SOURCE PASS:', counts)
