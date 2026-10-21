#!/usr/bin/env python3
"""Bounded original client popup paint;RGB1 and2physical-pixel ink limits."""
import argparse, ast, hashlib, json, math, random
from collections import Counter
from pathlib import Path
from PIL import Image, ImageChops, ImageFilter

ROOT = Path(__file__).resolve().parents[1]
p = argparse.ArgumentParser(); p.add_argument('--native-dir', required=True); p.add_argument('--log', required=True); p.add_argument('--output', required=True); p.add_argument('--fixture', default='tests/fixtures/status-clients-macos-source.json'); a = p.parse_args()
fixture_path=ROOT/a.fixture;fixture=json.loads(fixture_path.read_text())
metric=ROOT/'scripts/check-selector-font-macos-images.py';definition=next(v for v in ast.parse(metric.read_text()).body if isinstance(v,ast.FunctionDef)and v.name=='contour_distance');ns={'math':math};exec(compile(ast.Module(body=[definition],type_ignores=[]),str(metric),'exec'),ns)
def distance(a,b):
    if not a.getbbox()or not b.getbbox():return 0 if not a.getbbox()and not b.getbbox()else None
    for radius in range(4):
        ad=a.filter(ImageFilter.MaxFilter(radius*2+1))if radius else a;bd=b.filter(ImageFilter.MaxFilter(radius*2+1))if radius else b
        if not ImageChops.subtract(a,bd).getbbox()and not ImageChops.subtract(b,ad).getbbox():return radius
    return 4
rng=random.Random(20261021)
for _ in range(64):
    pa={(rng.randrange(12),rng.randrange(12))for _ in range(rng.randrange(1,35))};pb={(rng.randrange(12),rng.randrange(12))for _ in range(rng.randrange(1,35))};im=[]
    for points in [pa,pb]:
        mask=Image.new('L',(12,12))
        for point in points:mask.putpixel(point,255)
        im.append(mask)
    assert distance(*im)==ns['contour_distance'](list(pa),list(pb))
checks=[];raw=[];files=[];unpainted=[];sha=lambda path:hashlib.sha256(path.read_bytes()).hexdigest()

def sample(scene,name,images,x,y):
    at=(round(x*2),round(y*2))
    if not(0<=at[0]<images[0].width and 0<=at[1]<images[0].height):unpainted.append(dict(scene=scene,sample=name,physicalPoint=at));return
    values=[im.getpixel(at)for im in images];delta=max(abs(x-y)for x,y in zip(*values))
    checks.append(dict(scene=scene,sample=name,values=values,maximumDelta=delta,threshold=1,units='RGB',passed=delta<=1))

def ink(scene,name,images,rect):
    # Clip the original crop to the actual viewport;the narrow source popup
    # intentionally extends beyond the left edge. Unpainted space is not ink.
    x,y,w,h=rect;box=(max(0,round(x*2)),max(0,round(y*2)),min(images[0].width,round((x+w)*2)),min(images[0].height,round((y+h)*2)))
    if box[2]<=box[0]or box[3]<=box[1]:unpainted.append(dict(scene=scene,sample=name,physicalCrop=box));return
    masks=[];colors=[]
    for im in images:
        crop=im.crop(box);bg=crop.getpixel((0,0));bands=ImageChops.difference(crop,Image.new('RGB',crop.size,bg)).split();mask=ImageChops.lighter(ImageChops.lighter(bands[0],bands[1]),bands[2]).point(lambda v:255 if v>20 else 0);masks.append(mask)
        values=[v for v,on in zip(crop.get_flattened_data(),mask.get_flattened_data())if on];colors.append(Counter(values).most_common(1)[0][0]if values else None)
    delta=distance(*masks);checks.append(dict(scene=scene,sample=name+'.footprint',physicalCrop=box,pixels=[m.histogram()[255]for m in masks],maximumDelta=delta,threshold=2,units='physical pixels',passed=delta is not None and delta<=2))
    delta=max(abs(x-y)for x,y in zip(*colors))if all(v is not None for v in colors)else 0 if colors==[None,None]else None
    checks.append(dict(scene=scene,sample=name+'.foreground',values=colors,maximumDelta=delta,threshold=1,units='RGB',passed=delta is not None and delta<=1))

for row in fixture['records']:
    scene=row['name'];paths=[ROOT/row.get('archiveDirectory',fixture['archiveDirectory'])/(scene+'.png'),ROOT/a.native_dir/(scene+'.png')];images=[Image.open(v).convert('RGB')for v in paths];assert images[0].size==images[1].size
    files.extend(dict(path=str(v.relative_to(ROOT)),sha256=sha(v))for v in paths);popup=row['layout'][0];table=next(v for v in row['layout']if v['class']=='n-data-table-wrapper')
    sample(scene,'popup.top.fill',images,popup['x']+popup['w']/2,popup['y']+4)
    sample(scene,'table.left.border',images,table['x']+0.5,table['y']+16)
    sample(scene,'content.top-left.background',images,table['x']+0.5,table['y']+0.5)
    title=row['header'][0];button=row['buttons'][0];title_width=2*(button['x']-(title['x']+title['w']/2)+3)
    sample(scene,'title.fill',images,title['x']+4,title['y']+4)
    ink(scene,'title.text',images,(button['x']-12-title_width-2,title['y']+6,title_width+4,23))
    ink(scene,'refresh.svg',images,(button['x'],button['y'],button['w'],button['h']))
    for cell in row['header'][1:]:
        key=cell['key'];right=0 if key=='idle'else 1;sample(scene,'header.'+key+'.fill',images,cell['x']+4,cell['y']+4)
        sortable=bool(cell.get('sorter'));text_width=cell['w']-16-right-(19 if sortable else 0)
        ink(scene,'header.'+key+'.text',images,(cell['x']+8,cell['y']+6,text_width,21))
        if sortable:ink(scene,'header.'+key+'.sorter',images,(cell['x']+cell['w']-right-8-15,cell['y']+9,15,15))
    body=next(v for v in row['layout']if v['class']=='n-scrollbar-container')
    for i,data in enumerate(row['rows']):
        if len(data['cells'])!=5:continue
        for j,cell in enumerate(data['cells']):
            if body['y']<=cell['y']+4<body['y']+body['h']:
                sample(scene,f'row.{i}.{j}.fill',images,cell['x']+4,cell['y']+4)
            top=max(body['y'],cell['y']+6);bottom=min(body['y']+body['h'],cell['y']+cell['h']-6-(0 if i==len(row['rows'])-1 else 1))
            if bottom>top:ink(scene,f'row.{i}.{j}.text',images,(cell['x']+8,top,cell['w']-16-(1 if j<4 else 0),bottom-top))
    empty=next((v for v in row['layout']if v['class']=='n-empty__icon'),None)
    if empty:
        ink(scene,'empty.svg',images,(empty['x'],empty['y'],empty['w'],empty['h']))
        text=next(v for v in row['layout']if v['class']=='n-empty__description');ink(scene,'empty.description',images,(text['x'],text['y'],text['w'],text['h']))
    # Zero-error raw crop is independent of bounded text/fill checks.
    box=(max(0,round(popup['x']*2)),round((popup['y']-10)*2),min(images[0].width,round((popup['x']+popup['w'])*2)),round((popup['y']+popup['h'])*2));delta=ImageChops.difference(*[im.crop(box)for im in images]);count=sum(any(v)for v in delta.get_flattened_data());raw.append(dict(scene=scene,changedRGBPixels=count,maximumChannelDelta=max(v[1]for v in delta.getextrema()),threshold=0,passed=count==0))
report=dict(passed=all(v['passed']for v in checks),checks=checks,exactPopupCrops=raw,exactPopupCropsPassed=all(v['passed']for v in raw),files=files,unpaintedViewportSamples=unpainted,sourceFixtureSHA256=sha(fixture_path),logSHA256=sha(Path(a.log)),producerSHA256=sha(Path(__file__)),contourMetricSHA256=sha(metric),exactContourEquivalenceControls=64,scope=f'{len(fixture["records"])} current actual popup images:fixed background/border/fill samples andvisible text/refresh/sort/empty ink footprints andforeground. ExistingRGB1 and2physicalpx bounds. Raw wholepopup crops retained separately;no shadow/animation/loading/scroll/nativewindow acceptance.')
(ROOT/a.output).write_text(json.dumps(report,ensure_ascii=False,indent=2)+'\n');bad=[v for v in checks if not v['passed']];print('CLIENTS PAINT',len(checks),'checks;',len(bad),'differences;exact',sum(v['passed']for v in raw),'/',len(raw))
for v in bad[:12]:print(v)
raise SystemExit(0 if report['passed']else 1)
