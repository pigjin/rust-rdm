#!/usr/bin/env python3
"""Verify original editor references and the explicitly sized icon paths."""
import hashlib,json,re,subprocess,xml.etree.ElementTree as ET
from pathlib import Path

root=Path(__file__).resolve().parents[1]
original=Path('/Users/zlsj/Projects/tiny-rdm')
temporary=Path('/private/tmp/rust-rdm-source-qa')
commit=subprocess.check_output(['git','rev-parse','HEAD'],cwd=original,text=True).strip()
assert commit=='4ddb94a3c55f3401dbc66870f0d21cf0d6203cbc'
paths=['src/components/content_value/'+n+'.vue' for n in ['ContentEntryEditor','ContentValueHash','ContentValueList','ContentValueSet','ContentValueZSet','ContentEditor','FormatSelector']]+['src/stores/browser.js','src/stores/tab.js']
components=[]
for name in paths:
    data=(original/'frontend'/name).read_bytes()
    assert data==(temporary/name).read_bytes()==subprocess.check_output(['git','show','HEAD:frontend/'+name],cwd=original),name
    components.append({'path':name,'sha256':hashlib.sha256(data).hexdigest(),'unchanged':True})

def canonical(svg):
    element=ET.fromstring(svg)
    # In both renderers the control supplies its CSS/rendered size.
    element.attrib.pop('width',None);element.attrib.pop('height',None)
    return [(node.tag.split('}')[-1],dict(sorted(node.attrib.items()))) for node in element.iter()]

icons=[]
for name,vue,stroke,size in [('entry-pin','Pin',4,19),('entry-fullscreen','FullScreen',5,18),('entry-offscreen','OffScreen',5,18),('entry-close','WindowClose',5,18),('entry-editing','Edit',5,16)]:
    text=(original/f'frontend/src/components/icons/{vue}.vue').read_text()
    svg=re.search(r'<svg\b.*?</svg>',text,re.S).group()
    def bound_attrs(match):
        tag=match.group()
        for attr in ['stroke-width','fill','height','width']:
            if ':'+attr+'=' in tag:
                tag=re.sub(r'(?<![:\w-])'+attr+r'="[^"]*"\s*','',tag)
        return tag
    svg=re.sub(r'<[^>]+>',bound_attrs,svg)
    svg=svg.replace(':stroke-width="props.strokeWidth"',f'stroke-width="{stroke}"').replace(':fill="inverse ? \'currentColor\' : \'none\'"','fill="none"')
    svg=svg.replace(':height="props.size"','height="24"').replace(':width="props.size"','width="24"')
    assert canonical(svg)==canonical((root/f'assets/icons/{name}.svg').read_text()),name
    icons.append({'asset':name+'.svg','source':vue+'.vue','stroke':stroke,'controlSize':size,'matchesOriginalSizedPaths':True})
layout=json.loads((root/'tests/fixtures/edit-entry-layout-source.json').read_text())
interactions=json.loads((root/'tests/fixtures/entry-panel-interactions-source.json').read_text())
assert len(layout['records'])==13 and len(interactions['checks'])==8
out={'sourceCommit':commit,'unchangedFrontendFiles':components,'icons':icons,'layoutScenes':13,'interactionChecks':8,'themeSettleMilliseconds':1600,
     'scope':'Original component/store files match the pinned commit and sampling copy. Five SVG path trees and viewboxes match explicit stroke props; intrinsic size/namespace normalized because each control supplies its rendered size. Named bounds and settled-theme colors; no whole-image, animation or all-language claim.','passed':True}
(root/'outputs/entry-panel-source-check.json').write_text(json.dumps(out,ensure_ascii=False,indent=2)+'\n')
print('SOURCE ENTRY PANEL PROVENANCE PASS: 9 files, 5 sized icon trees, 13 layouts and 8 interaction checks')
