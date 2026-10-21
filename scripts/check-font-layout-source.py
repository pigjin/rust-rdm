#!/usr/bin/env python3
"""Check macOS reference-engine font evidence and retained CoreText tracking."""
import hashlib
import json
from pathlib import Path

ROOT = Path(__file__).resolve().parents[1]
def read(name):
    return json.loads((ROOT / name).read_text())
def digest(name):
    return hashlib.sha256((ROOT / name).read_bytes()).hexdigest()
def index(records):
    return {(r['family'], r['fontSize'], r['text']): r for r in records}

chromium = read('outputs/font-advance-source.json')['records']
webkit = read('outputs/font-advance-webkit.json')['records']
monaco_report = read('outputs/font-advance-webkit-monaco-source.json')
monaco = monaco_report['records']
fixture = read('tests/fixtures/font-layout-macos-source.json')
core = read('outputs/font-advance-coretext.json')
tracking = read('outputs/font-tracking-coretext.json')
assert [len(chromium), len(webkit), len(monaco), len(fixture['samples']), len(core), len(tracking)] == [280, 280, 308, 231, 40, 56]
assert monaco_report['editorRecord']['viewport'] == [1024, 768]
assert monaco_report['editorRecord']['layout']['width'] == 600
assert monaco_report['editorRecord']['layout']['height'] == 300
assert monaco_report['editorRecord']['layout']['contentWidth'] > 0
assert fixture['rawReportSha256'] == digest(fixture['rawReport'])
assert fixture['controlledEditorSize'] == [600, 300]
expected_fixture = [{'family': r['family'], 'fontSize': r['fontSize'], 'text': r['text'], 'width': r['domWidth']}
                    for r in monaco if r['family'] in ['Menlo', 'Monaco', 'Courier New']]
assert fixture['samples'] == expected_fixture
c, w, m = index(chromium), index(webkit), index(monaco)
for r in expected_fixture:
    key = (r['family'], r['fontSize'], r['text'])
    actual = m[key]
    assert actual['lineText'].replace(chr(160), ' ') == r['text']
    assert abs(actual['canvasWidth'] - r['width']) < 0.02, key
    if key in w:
        assert abs(w[key]['canvasWidth'] - r['width']) < 0.02, key
checks = []
for size, native_width, chromium_width in [(12,16,15),(13,17,16),(14,19,18),(15,20,19),(18,22,21),(24,25,24),(32,32,32)]:
    for family in ['Menlo', 'Monaco', 'Courier New']:
        assert m[(family,size,'🙂')]['domWidth'] == native_width
        assert m[(family,size,'🙂🙂')]['domWidth'] == native_width * 2
        assert w[(family,size,'🙂')]['canvasWidth'] == native_width
        assert c[(family,size,'🙂')]['canvasWidth'] == chromium_width
    native = next(r for r in tracking if r['family']=='Menlo' and r['size']==size and r['attrs']=={})
    assert abs(native['width']-w[('Menlo',size,'中文🙂  alpha')]['canvasWidth']) < 0.02
    checks.append({'fontSize':size,'macOSWKWebViewEmojiAdvance':native_width,'chromiumEmojiAdvance':chromium_width})
controls = ['A中B','Ae'+chr(0x301)+'B','AΩ·B']
for text in controls:
    native = next(r for r in core if r['family']=='Menlo' and r['text']==text)
    assert abs(native['width']-m[('Menlo',14,text)]['domWidth']) < 0.02
patch = ROOT/'vendor/gpui-apple'
upstream = Path('/private/tmp/rust-rdm-cargo/registry/src/index.crates.io-1949cf8c6b5b557f/gpui-pre-apple-0.3.9')
marker = '\n#[cfg(test)]\nmod tests {'
production = (patch/'src/text_system.rs').read_text().split(marker,1)[0]
assert 'NSCTFontUnscaledTrackingAttribute' not in production
assert 'rdm_emoji_advances_match_macos_webkit' in (patch/'src/text_system.rs').read_text()
changed, unchanged = [], []
for path in sorted(patch.rglob('*')):
    if not path.is_file() or path.name=='RDM-CHANGES.md': continue
    relative=path.relative_to(patch);target=upstream/relative
    assert target.is_file(), relative
    (unchanged if path.read_bytes()==target.read_bytes() else changed).append(str(relative))
assert changed==['Cargo.toml','src/text_system.rs']
assert '93d745668805029476e6d7fbb19652116fed51de' in (patch/'RDM-CHANGES.md').read_text()
assert 'v2 v2.13.0' in Path('/Users/zlsj/Projects/tiny-rdm/go.mod').read_text()
files = ['outputs/font-advance-source.json','outputs/font-advance-webkit.json','outputs/font-advance-webkit-monaco-source.json','outputs/font-advance-coretext.json','outputs/font-tracking-coretext.json','tests/fixtures/font-layout-macos-source.json',
         'outputs/source-probes/font-advance-source.mjs','outputs/source-probes/font-advance-webkit.swift','outputs/source-probes/font-advance-webkit-monaco.swift','outputs/source-probes/font-advance-webkit-monaco.js',
         'vendor/gpui-apple/Cargo.toml','vendor/gpui-apple/src/text_system.rs','vendor/gpui-apple/RDM-CHANGES.md','assets/licenses/GPUI-Apple-LICENSE.txt']
report = {'scope':'Independent font advances only: standalone Chromium and non-persistent macOS WKWebView, plus the original Monaco component with controlled 600x300 layout and wrapping disabled. Does not verify original full-page geometry, Wails RPC, glyph bitmaps, antialiasing, native-window interaction or other operating systems.',
          'sourceCommit':fixture['sourceCommit'],'originalWailsVersion':'2.13.0','macOSReferenceEngine':'WKWebView',
          'primarySource':'https://github.com/wailsapp/wails/blob/v2.13.0/v2/internal/frontend/desktop/darwin/WailsContext.m',
          'chromiumRecords':280,'webkitStandaloneRecords':280,'webkitMonacoRecords':308,'includedGPUIWidthSamples':231,'baselineCoreTextRecords':40,'trackingExperimentRecords':56,
          'sizeChecks':checks,'controlStrings':controls,'macOS14pxEmojiAdvance':19,'chromium14pxEmojiAdvance':18,
          'chromiumTrackingCandidateAcceptedForMacOS':False,'candidateReport':'font-scroll-chromium-candidate-check.json',
          'implementation':'Retain CoreText shaping, rasterization and optical tracking. Production now selects the system UI CSS weight axis and caches complete native font instances. The separate font-weight report covers that change; this report retains the original normal-weight advance evidence and unchanged tolerance.',
          'vendor':{'crate':'gpui-pre-apple','version':'0.3.9','zedRevision':'93d745668805029476e6d7fbb19652116fed51de','productionTextSystemIdenticalToUpstream':production == (upstream/'src/text_system.rs').read_text().split(marker,1)[0],'modifiedUpstreamFiles':changed,'unchangedUpstreamFiles':unchanged},
          'files':[{'path':p,'sha256':digest(p)} for p in files],'passed':True}
(ROOT/'outputs/font-layout-source-check.json').write_text(json.dumps(report,ensure_ascii=False,indent=2)+'\n')
print('MACOS FONT SOURCE PASS: 280 blank WKWebView + 308 original Monaco records; 231 normal-weight GPUI source widths; CoreText tracking retained; variable-font production change recorded separately')
