#!/usr/bin/env python3
"""Verify original nested-preferences layout and keyboard evidence."""
import hashlib
import json
import subprocess
from pathlib import Path

root = Path(__file__).resolve().parents[1]
original = Path('/Users/zlsj/Projects/tiny-rdm')
temporary = Path('/private/tmp/rust-rdm-source-qa')
commit = subprocess.check_output(['git', 'rev-parse', 'HEAD'], cwd=original, text=True).strip()
assert commit == '4ddb94a3c55f3401dbc66870f0d21cf0d6203cbc'
base = json.loads((root / 'outputs/entry-picker-source-check.json').read_text())
assert base['passed'] and base['sourceCommit'] == commit
paths = [item['path'] for item in base['frontendFiles']] + [
    'src/components/dialogs/PreferencesDialog.vue', 'src/components/dialogs/DecoderDialog.vue',
    'src/AppContent.vue', 'src/components/content_value/ContentValueWrapper.vue']
files = []
for path in paths:
    data = (original / 'frontend' / path).read_bytes()
    assert data == (temporary / path).read_bytes()
    assert data == subprocess.check_output(['git', 'show', 'HEAD:frontend/' + path], cwd=original)
    files.append({'path': path, 'sha256': hashlib.sha256(data).hexdigest(), 'unchanged': True})
fixture = json.loads((root / 'tests/fixtures/entry-preferences-source.json').read_text())
assert len(fixture['records']) == 5 and len(fixture['events']) == 6
for record in fixture['records']:
    assert record['focusOwner'] == 'BODY'
for event in fixture['events']:
    if event['name'].endswith('shortcut-under-preferences'):
        assert event['requests'] == [] and event['editorOpen'] and event['preferencesOpen']
report = {'sourceCommit': commit, 'frontendFiles': files, 'layoutScenes': 5,
          'interactionChecks': 6, 'scope': fixture['scope'], 'passed': True}
(root / 'outputs/entry-preferences-source-check.json').write_text(json.dumps(report, ensure_ascii=False, indent=2) + '\n')
print('SOURCE ENTRY PREFERENCES PASS: 15 unchanged files, five layouts, six cancel/focus/save/shortcut samples')
