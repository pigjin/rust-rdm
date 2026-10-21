#!/usr/bin/env python3
"""Compare actual controlled status frames/tick callbacks to original WK records."""
import argparse
import hashlib
import json
from pathlib import Path

ROOT=Path(__file__).resolve().parents[1]
p=argparse.ArgumentParser()
p.add_argument('--log',required=True)
p.add_argument('--fixture',default='tests/fixtures/status-layout-macos-source.json')
p.add_argument('--output',default='outputs/status-layout-macos-layout-check.json')
args=p.parse_args()
log=Path(args.log)
fixture_path=ROOT/args.fixture
fixture=json.loads(fixture_path.read_text())
frames={};ticks={};active=None;pending={};last={}
for line in log.read_text().splitlines():
    if line.startswith('STATUS LAYOUT START: '):
        active=line.split(': ',1)[1];pending={};last={}
    elif active and line.startswith('STATUS AXIS TICK: '):
        row=json.loads(line.split(': ',1)[1]);color=row['chart']
        group=pending.setdefault(color,[]);group.append(row)
        if row['value']==0:
            last[color]=group[:];pending[color]=[]
    elif line.startswith('STATUS LAYOUT FRAME: '):
        row=json.loads(line.split(': ',1)[1]);frames[row['scene']]={v['id']:v for v in row['frames']}
        ticks[row['scene']]=last;active=None
checks=[]

def geometry(scene,id,actual,expected):
    for key in ['x','y','w','h']:
        delta=abs(actual[key]-expected[key])
        checks.append({'scene':scene,'element':id,'measurement':key,'actual':actual[key],'expected':expected[key],'maximumDelta':delta,'threshold':0.6,'units':'logical pixels','passed':delta<0.6})

for row in fixture['records']:
    scene=row['name'];native=frames[scene];layout=row['layout']
    header=[v for v in layout if v['class']in ['n-card-header__main','n-card-header__extra']]
    header_rect={'x':min(v['x']for v in header),'y':min(v['y']for v in header)}
    header_rect['w']=max(v['x']+v['w']for v in header)-header_rect['x']
    header_rect['h']=max(v['y']+v['h']for v in header)-header_rect['y']
    cards=[v for v in layout if 'n-card--embedded'in v['class']]
    cells=[v for v in layout if v['class']=='line-chart-item']
    source={'status-root':layout[0],'status-summary-card':cards[0],'status-header-content':header_rect,'status-summary-grid':next(v for v in layout if v['class']=='n-grid'),'status-activity-card':cards[1]}
    for color,cell in zip(['ff6384','ff9f40','9966ff','4bc0c0'],cells):source['status-chart-slot-'+color]=cell
    for stat in row['stats']:
        i=stat['index'];source[f'status-stat-{i}']=stat['outer']
        source[f'status-stat-label-{i}']=next(v for v in stat['children']if v['class']=='n-statistic__label')
        source[f'status-stat-row-{i}']=next(v for v in stat['children']if v['class']=='n-statistic-value')
        value=next(v['text']for v in stat['children']if v['class']=='n-statistic-value__content')
        if i in [0,3]:value+=' '+next(v['text']for v in stat['children']if v['class']=='n-statistic-value__suffix')
        actual=native[f'status-value-{i}']['label']
        checks.append({'scene':scene,'element':f'status-value-{i}','actual':actual,'expected':value,'passed':actual==value,'scope':'Accessible numeric/unit value only;separate child typography and painted pixels are not accepted here.'})
    if 'headerDetails' in row and 'status-server-name' in native:
        details=row['headerDetails'];name=details['serverText'];main=next(v for v in layout if v['class']=='n-card-header__main')
        source['status-server-name']={'x':name['x'],'y':main['y'],'w':name['w'],'h':main['h']}
        source['status-refresh']=details['refresh']
        for key,tag in zip(['version','mode','role'],details['tags']):source['status-tag-'+key]=tag['outer']
        for id,tab in zip(['activity','status-info'],details['tabs']):source[id]=tab['outer']
        if not details['tags']:
            checks.append({'scene':scene,'element':'empty-header-tags','passed':not any(k.startswith('status-tag-')for k in native),'scope':'Original empty-info header omits all three tags.'})
        source['show-clients']=next(v for v in row['stats'][1]['children']if 'n-button--small-type'in v['class'])
    for id,expected in source.items():geometry(scene,id,native[id],expected)
    for index,color in enumerate(['ff6384','ff9f40','9966ff','4bc0c0']):
        actual=list(reversed(ticks[scene][color]))
        expected=row['charts'][index]['scales']['y']['ticks']
        values=[v['value']for v in actual];expected_values=[v['value']for v in expected]
        labels=[v['label']for v in actual];expected_labels=[v['label']for v in expected]
        delta=max((abs(a-b)for a,b in zip(values,expected_values)),default=0)if len(values)==len(expected_values)else None
        checks.append({'scene':scene,'element':'y-axis-'+color,'actualValues':values,'expectedValues':expected_values,'actualLabels':labels,'expectedLabels':expected_labels,'maximumDelta':delta,'threshold':1e-6,'units':'data units','passed':delta is not None and delta<=1e-6 and labels==expected_labels})
report={'passed':all(v['passed']for v in checks),'scenes':len(fixture['records']),'checks':checks,'logSHA256':hashlib.sha256(log.read_bytes()).hexdigest(),'sourceFixtureSHA256':hashlib.sha256(fixture_path.read_bytes()).hexdigest(),'producerSHA256':hashlib.sha256(Path(__file__).read_bytes()).hexdigest(),'scope':'Actual GPUI layout boxes and last complete actual tick-formatter callback batches before each frame;original0.6 logical-pixel layout limit and exact labels/1e-6 data units. Header target is the union of original main/extra DOM rectangles. No whole-image/native or curve/paint acceptance.'}
(ROOT/args.output).write_text(json.dumps(report,ensure_ascii=False,indent=2)+'\n')
bad=[v for v in checks if not v['passed']]
print('STATUS LAYOUT:',len(checks),'checks;',len(bad),'differences;numeric accessible labels failed',sum(not v['passed']for v in checks if v['element'].startswith('status-value-')))
for row in bad[:8]:print(row)
raise SystemExit(0 if report['passed']else 1)
