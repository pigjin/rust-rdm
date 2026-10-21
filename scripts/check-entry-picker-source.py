#!/usr/bin/env python3
"""Check the provenance of collection-editor dropdown samples."""
import hashlib
import json
import subprocess
from pathlib import Path

root = Path(__file__).resolve().parents[1]
original = Path('/Users/zlsj/Projects/tiny-rdm')
temporary = Path('/private/tmp/rust-rdm-source-qa')
commit = subprocess.check_output(['git', 'rev-parse', 'HEAD'], cwd=original, text=True).strip()
assert commit == '4ddb94a3c55f3401dbc66870f0d21cf0d6203cbc'
paths = ['src/components/content_value/' + name + '.vue' for name in
         ['ContentEntryEditor', 'ContentValueHash', 'ContentValueList', 'ContentValueSet',
          'ContentValueZSet', 'ContentEditor', 'FormatSelector']]
paths += ['src/components/common/DropdownSelector.vue', 'src/stores/browser.js',
          'src/stores/tab.js', 'src/stores/preferences.js']
files = []
for path in paths:
    data = (original / 'frontend' / path).read_bytes()
    assert data == (temporary / path).read_bytes()
    assert data == subprocess.check_output(['git', 'show', 'HEAD:frontend/' + path], cwd=original)
    files.append({'path': path, 'sha256': hashlib.sha256(data).hexdigest(), 'unchanged': True})
backend = []
for path in ['backend/services/preferences_service.go', 'backend/utils/convert/convert.go']:
    data = (original / path).read_bytes()
    assert data == subprocess.check_output(['git', 'show', 'HEAD:' + path], cwd=original)
    backend.append({'path': path, 'sha256': hashlib.sha256(data).hexdigest(), 'unchanged': True})
converter = (original / 'backend/utils/convert/convert.go').read_text().split('var BuildInDecoders =', 1)[1].split('}', 1)[0]
assert 'DECODE_NONE' not in converter
fixture = json.loads((root / 'tests/fixtures/entry-picker-source.json').read_text())
assert len(fixture['records']) == 12 and len(fixture['events']) == 8
assert len({record['name'] for record in fixture['records']}) == 12
report = {'sourceCommit': commit, 'frontendFiles': files, 'backendFiles': backend,
          'layoutScenes': 12, 'interactionChecks': 8, 'builtinNoneExcludedAsOriginal': True,
          'scope': fixture['scope'], 'passed': True}
(root / 'outputs/entry-picker-source-check.json').write_text(json.dumps(report, ensure_ascii=False, indent=2) + '\n')
print('SOURCE ENTRY PICKER PASS: 11 unchanged frontend files, 2 Go files, 12 layouts and 8 interaction samples')
