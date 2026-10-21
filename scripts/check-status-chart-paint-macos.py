#!/usr/bin/env python3
"""Bounded actual status chart paint using the existing RGB1 / 2px limits.

Solid stroke masks, legend fill samples and text footprints only. Raster alpha
edges and complete chart/page equality are checked separately and may fail.
"""
import argparse,ast,hashlib,json,math
from collections import Counter
from pathlib import Path
from PIL import Image,ImageChops
ROOT=Path(__file__).resolve().parents[1]
p=argparse.ArgumentParser();p.add_argument('--log',required=True);p.add_argument('--native-dir',default='outputs/ui-qa');p.add_argument('--output',required=True);a=p.parse_args()
fpath=ROOT/'tests/fixtures/status-layout-macos-source.json';fixture=json.loads(fpath.read_text());checks=[];files=[]
# Reuse the prior physical-pixel contour metric without changing its search or limit.
metric=ROOT/'scripts/check-selector-font-macos-images.py';tree=ast.parse(metric.read_text());definition=next(v for v in tree.body if isinstance(v,ast.FunctionDef)and v.name=='contour_distance');namespace={'math':math};exec(compile(ast.Module(body=[definition],type_ignores=[]),str(metric),'exec'),namespace);distance=namespace['contour_distance']
def sha(p):return hashlib.sha256(p.read_bytes()).hexdigest()
def positions(mask):return [(i%mask.width,i//mask.width)for i,v in enumerate(mask.get_flattened_data())if v]
def color_mask(im,color,mode):
 d=ImageChops.difference(im,Image.new('RGB',im.size,color));bands=d.split();maximum=ImageChops.lighter(ImageChops.lighter(bands[0],bands[1]),bands[2]);return maximum.point(lambda v:255 if (v<=1 if mode=='solid'else v>20)else 0)
def contour(scene,name,first,second):
 d=0 if not first and not second else distance(first,second)
 checks.append({'scene':scene,'sample':name,'sourcePixels':len(first),'nativePixels':len(second),'maximumDelta':d,'threshold':2,'units':'physical pixels','passed':d is not None and d<=2})
def text(scene,name,first,second,bg):
 m1=color_mask(first,bg,'ink');m2=color_mask(second,bg,'ink');contour(scene,name+'.footprint',positions(m1),positions(m2))
 colors=[]
 for im,mask in [(first,m1),(second,m2)]:
  values=[rgb for rgb,ink in zip(im.get_flattened_data(),mask.get_flattened_data())if ink];colors.append(Counter(values).most_common(1)[0][0]if values else None)
 delta=0 if colors==[None,None]else max(abs(x-y)for x,y in zip(*colors))if all(c is not None for c in colors)else None
 checks.append({'scene':scene,'sample':name+'.foreground','values':colors,'maximumDelta':delta,'threshold':1,'units':'RGB','passed':delta is not None and delta<=1})
for row in fixture['records']:
 scene=row['name'];paths=[ROOT/fixture['archiveDirectory']/(scene+'.png'),ROOT/a.native_dir/(scene+'.png')];images=[Image.open(v).convert('RGB')for v in paths];files.extend({'path':str(v.relative_to(ROOT)),'sha256':sha(v)}for v in paths);bg=(33,33,33)if row['configuration']['theme']=='dark'else(250,250,250)
 for ci,c in enumerate(row['charts']):
  q=c['canvas'];area=c['chartArea'];x=q['x'];y=q['y'];name=f'chart-{ci}'
  def crop(rect):
   l,t,r,b=rect;box=(math.floor((x+l)*2),math.floor((y+t)*2),math.ceil((x+r)*2),math.ceil((y+b)*2));return [im.crop(box)for im in images]
  text(scene,name+'.y-labels',*crop((0,area['top']-10,area['left']-9,area['bottom']+10)),bg)
  text(scene,name+'.x-labels',*crop((0,area['bottom']+9,c['width'],c['height'])),bg)
  canvases=crop((0,0,c['width'],c['height']))
  for i,(box,item) in enumerate(zip(c['legend']['hitBoxes'],c['legend']['legendItems'])):
   text(scene,f'{name}.legend-{i}',*crop((box['left']+45,box['top']-3,box['left']+box['width']+2,box['top']+15)),bg)
   sample=(round((x+box['left']+20)*2),round((y+box['top']+6)*2));values=[im.getpixel(sample)for im in images];delta=max(abs(v-w)for v,w in zip(*values));checks.append({'scene':scene,'sample':f'{name}.legend-{i}.fill','values':values,'maximumDelta':delta,'threshold':1,'units':'RGB','passed':delta<=1})
   target=tuple(int(v)for v in item['strokeStyle'].removeprefix('rgb(').removesuffix(')').split(','))
   contour(scene,f'{name}.series-{i}.solid-stroke',*[positions(color_mask(im,target,'solid'))for im in canvases])
report={'passed':all(v['passed']for v in checks),'checks':checks,'scenes':10,'charts':40,'files':files,'logSHA256':sha(Path(a.log)),'sourceFixtureSHA256':sha(fpath),'producerSHA256':sha(Path(__file__)),'contourMetricSHA256':sha(metric),'scope':'Actual PNG text ink footprints/foreground,solid stroke masks including legend and plot,and fixed legend fill centers. Same prior RGB1 and two-physical-pixel contour limits;not antialias equality,all fill pixels,whole-page/native or full functional acceptance.'};(ROOT/a.output).write_text(json.dumps(report,ensure_ascii=False,indent=2)+'\n');bad=[v for v in checks if not v['passed']];print('STATUS CHART PAINT',len(checks),'checks;',len(bad),'differences');
for v in bad[:10]:print(v)
raise SystemExit(1 if bad else 0)
