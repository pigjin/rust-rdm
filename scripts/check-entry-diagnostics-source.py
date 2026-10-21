#!/usr/bin/env python3
"""Verify provenance of original Monaco diagnostic and token-color samples."""
import hashlib
import json
import subprocess
from urllib.parse import unquote
import xml.etree.ElementTree as ET
from pathlib import Path

root = Path(__file__).resolve().parents[1]
original = Path('/Users/zlsj/Projects/tiny-rdm')
temporary = Path('/private/tmp/rust-rdm-source-qa')
commit = subprocess.check_output(['git', 'rev-parse', 'HEAD'], cwd=original, text=True).strip()
assert commit == '4ddb94a3c55f3401dbc66870f0d21cf0d6203cbc'
base = json.loads((root / 'outputs/entry-picker-source-check.json').read_text())
assert base['passed'] and base['sourceCommit'] == commit
paths = list(dict.fromkeys([item['path'] for item in base['frontendFiles']] + ['src/utils/monaco.js', 'package.json']))
files = []
for path in paths:
    data = (original / 'frontend' / path).read_bytes()
    assert data == (temporary / path).read_bytes()
    assert data == subprocess.check_output(['git', 'show', 'HEAD:frontend/' + path], cwd=original)
    files.append({'path': path, 'sha256': hashlib.sha256(data).hexdigest(), 'unchanged': True})
fixture = json.loads((root / 'tests/fixtures/entry-diagnostics-source.json').read_text())
assert fixture['sourceCommit'] == commit and len(fixture['records']) == 12
assert [len(record['markers']) for record in fixture['records']] == [1,1,1,1,1,1,1,2,1,0,0,1]
for record in fixture['records']:
    assert record['language'] == 'json' and record['value'] == record['input']
    for marker in record['markers']:
        assert marker['severity'] == 8
        assert marker['startLineNumber'] >= 1 and marker['startColumn'] >= 1
        assert marker['message'] and marker['code'] is not None
assert fixture['records'][0]['markers'][0]['endColumn'] == 8
assert fixture['records'][11]['markers'][0]['code'] == '521'
package = json.loads((temporary / 'node_modules/monaco-editor/package.json').read_text())
assert package['version'] == '0.47.0'
extended = json.loads((root / 'tests/fixtures/json-diagnostics-extended-source.json').read_text())
assert extended['sourceCommit'] == commit and len(extended['records']) == 65
for record in extended['records']:
    assert record['language'] == 'json'
    assert record['value'] == record['input'].removeprefix('\ufeff')
    assert len(record['decorations']) == len(record['markers'])
    assert all(marker['severity'] in [4, 8] for marker in record['markers'])
assert sum(marker['severity'] == 4 for record in extended['records'] for marker in record['markers']) == 10
hover = json.loads((root / 'tests/fixtures/json-diagnostic-hover-source.json').read_text())
assert hover['sourceCommit'] == commit and len(hover['records']) == 2
for record in hover['records']:
    assert record['fontInfo']['fontSize'] == 14 and record['fontInfo']['lineHeight'] == 21
    assert record['fontInfo']['fontFamily'].startswith('Menlo, Monaco,')
    assert record['hover']['width'] == 289 and record['hover']['height'] == 75
    assert record['rulerPixels']['redBounds'] == [10, 4, 19, 41]
    assert record['padding'] == {'top': 0, 'bottom': 0} and record['topLine'] == 0
    for name in [record['name'], record['editorFrame']]:
        assert (root / 'outputs/ui-reference' / (name + '.png')).is_file()
tile = ET.fromstring((root / 'assets/icons/editor-diagnostic-wave.svg').read_text())
assert tile.attrib['viewBox'] == '0 0 6 3'
for record in hover['records']:
    svg = ET.fromstring(unquote(record['squiggle'][0]['background'].split(',', 1)[1].rsplit('"', 1)[0]))
    assert [el.attrib['points'] for el in svg.iter() if el.tag.endswith('polygon')] == [el.attrib['points'] for el in tile.iter() if el.tag.endswith('polygon')]
render = json.loads((root / 'tests/fixtures/json-diagnostic-render-source.json').read_text())
assert render['sourceCommit'] == commit and len(render['records']) == 4
for record in render['records']:
    assert record['input'] == record['value'] if 'input' in record else record['text'] == record['value']
    assert record['language'] == 'json' and record['markers'] and record['decorations']
    assert (root / 'outputs/ui-reference' / (record['name'] + '.png')).is_file()
assert len(render['records'][-1]['decorations']) == 5 and len(render['records'][-1]['lines']) == 5
interaction = json.loads((root / 'tests/fixtures/json-diagnostic-hover-interaction-source.json').read_text())
assert interaction['sourceCommit'] == commit and len(interaction['records']) == 4
for record in interaction['records']:
    assert record['model']['value'] == '/*' and len(record['model']['markers']) == 2
    assert record['model']['hoverOption'] == {'enabled': True, 'delay': 300, 'hidingDelay': 300, 'sticky': True, 'above': True}
    if 'overlap' in record['name']:
        assert 'Comments are not permitted in JSON.' in record['text']
        assert record['bounds']['height'] == 126
    else:
        assert '1 of 2 problems' in record['text'] and record['bounds']['height'] == 84
    assert (root / 'outputs/ui-reference' / (record['name'] + '.png')).is_file()
characters = json.loads((root / 'tests/fixtures/editor-character-layout-source.json').read_text())
assert characters['sourceCommit'] == commit and len(characters['records']) == 7
for record in characters['records']:
    assert record['input'] == record['value']
    assert len(record['columns']) == record['utf16Length'] + 1
    assert record['fontInfo']['fontFamily'].startswith('Menlo, Monaco,') and record['fontInfo']['fontSize'] == 14
    assert record['columns'][0] == {'column': 1, 'x': 0}
layouts = json.loads((root / 'tests/fixtures/json-hover-layout-source.json').read_text())
assert layouts['sourceCommit'] == commit and len(layouts['records']) == 4
assert [(r['hover']['width'], r['hover']['height']) for r in layouts['records']] == [(429,54),(218,54),(268,54),(361,84)]
for record in layouts['records']:
    assert (root / 'outputs/ui-reference' / (record['name'] + '.png')).is_file()
navigation = json.loads((root / 'tests/fixtures/json-navigation-behavior-source.json').read_text())
assert navigation['sourceCommit'] == commit and len(navigation['events']) == 10
assert [event['name'] for event in navigation['events']] == ['keyboard-open','next','next-wrap','previous-wrap','escape-close','keyboard-reopen','edited-before-validation','edited-valid-stale','next-without-markers','final-close']
assert navigation['events'][7]['stale'] and navigation['events'][8]['stale']
assert not navigation['events'][-1]['visible']
selection = json.loads((root / 'tests/fixtures/json-diagnostic-selection-source.json').read_text())
assert selection['sourceCommit'] == commit and len(selection['records']) == 10
for record in selection['records']:
    assert record['selected'] and record['copyEvent']['selected'] == record['selected']
    assert not record['copyEvent']['providedText'] and not record['editorTextFocused']
    assert record['gesture']['kind'] in ['drag', 'double-click'] and record['layout']['selectedRects']
    assert (root / 'outputs/ui-reference' / ('json-select-' + record['name'] + '.png')).is_file()
assert len(selection['focusEvents']) == 3 and all(event['editorTextFocused'] for event in selection['focusEvents'])
assert not selection['focusEvents'][0]['hoverVisible']
assert '2 of 2 problems' in selection['focusEvents'][1]['navigationText']
assert not selection['focusEvents'][2]['navigationVisible']
scroll = json.loads((root / 'tests/fixtures/json-diagnostic-scroll-source.json').read_text())
assert scroll['sourceCommit'] == commit and len(scroll['records']) == len(scroll['events']) == 4
assert [e.get('scrollTop') for e in scroll['events']] == [521, 1041, None, 1041]
assert not scroll['events'][2]['visible']
for index, record in enumerate(scroll['records']):
    content = next(m for m in record['metrics'] if m['class'] == 'monaco-hover-content')
    assert record['editorValue'] == '/*' and record['editorScrollTop'] == 0
    assert (content['clientHeight'], content['scrollHeight']) == (250, 1291)
    assert content['scrollTop'] == [0, 25, 1041, 1041][index]
    assert record['occlusions'][0]['elements'][0]['class'] == 'n-tabs-nav-scroll-content'
    assert (root / 'outputs/ui-reference' / (record['name'] + '.png')).is_file()
assert (root / 'assets/fonts/monaco-codicon.ttf').read_bytes() == (temporary / 'node_modules/monaco-editor/esm/vs/base/browser/ui/codicons/codicon/codicon.ttf').read_bytes()
subprocess.run(['python3', str(root / 'scripts/extract-monaco-diagnostic-icons.py'), '--check'], check=True, capture_output=True)
libraries = []
for path in ['package.json', 'esm/vs/language/json/json.worker.js', 'esm/vs/language/json/jsonMode.js',
             'esm/vs/editor/browser/viewParts/overviewRuler/decorationsOverviewRuler.js',
             'esm/vs/editor/common/core/editorColorRegistry.js',
             'esm/vs/editor/contrib/hover/browser/markerHoverParticipant.js',
             'esm/vs/editor/contrib/gotoError/browser/gotoErrorWidget.js',
             'esm/vs/editor/common/viewLayout/viewLineRenderer.js',
             'esm/vs/editor/contrib/gotoError/browser/gotoError.js',
             'esm/vs/editor/contrib/gotoError/browser/markerNavigationService.js',
             'esm/vs/editor/contrib/zoneWidget/browser/zoneWidget.js',
             'esm/vs/editor/contrib/hover/browser/contentHover.js',
             'esm/vs/base/browser/ui/hover/hoverWidget.js',
             'esm/vs/base/browser/ui/scrollbar/scrollbarState.js',
             'esm/vs/base/browser/ui/scrollbar/scrollableElement.js',
             'esm/vs/base/browser/ui/codicons/codicon/codicon.ttf']:
    data = (temporary / 'node_modules/monaco-editor' / path).read_bytes()
    libraries.append({'path': path, 'sha256': hashlib.sha256(data).hexdigest()})
report = {'sourceCommit': commit, 'frontendFiles': files, 'monacoVersion': package['version'],
          'installedMonacoFiles': libraries, 'samples': 77,
          'markers': sum(len(record['markers']) for data in [fixture, extended] for record in data['records']),
          'hoverLayouts': 2, 'editorFrames': 6, 'additionalRenderSamples': 4,
          'squiggleSVGGeometryMatchesOriginal': True,
          'sourceOnlyHoverInteractions': 4, 'sourceOnlyCharacterLayouts': 7,
          'actualViewProblemOpenAndCloseVerifiedInSource': True,
          'additionalHoverLayouts': 4, 'navigationBehaviorEvents': 10, 'bundledCodiconBytesMatchOriginal': True,
          'selectionSourceRecords': 10, 'selectionSourceFocusEvents': 3,
          'hoverScrollSourceRecords': 4, 'hoverScrollSourceDragEvents': 4,
          'sourceSelectionCopyDefaultPreventedToProtectClipboard': True,
          'nativeHoverNavigationParityVerified': False,
          'verificationScope': [fixture['scope'], extended['scope'], hover['scope'], render['scope'], interaction['scope'], characters['scope'], layouts['scope'], navigation['scope'], selection['scope'], scroll['scope']],
          'nativeDiagnosticParityVerified': False, 'passed': True}
(root / 'outputs/entry-diagnostics-source-check.json').write_text(json.dumps(report, indent=2) + '\n')
print('SOURCE DIAGNOSTICS PASS:', len(files), 'unchanged frontend/manifest files; 77 model samples, two hover layouts, six editor frames and original SVG geometry; native alignment checked separately')
