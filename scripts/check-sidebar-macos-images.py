#!/usr/bin/env python3
"""Bounded sidebar/header/tag and settled4px divider paint; not full image parity."""
import argparse,hashlib,json,math
from collections import Counter
from pathlib import Path
from PIL import Image
ROOT=Path(__file__).resolve().parents[1]
p=argparse.ArgumentParser();p.add_argument('--log',required=True);p.add_argument('--output',default='outputs/sidebar-macos-image-check.json');args=p.parse_args();log=Path(args.log);frames={}
for line in log.read_text().splitlines():
    if line.startswith(('SIDEBAR RESIZE FRAME: ','SIDEBAR CONTRACT FRAME: ')):
        v=json.loads(line.split(': ',1)[1]);frames[v['scene']]={z['id']:z for z in v['frames']}
records=[];files=[log,Path(__file__)];checks=[];skips=[]
for name in ['sidebar-resize-macos-source.json','sidebar-contract-macos-source.json']:
    path=ROOT/'tests/fixtures'/name;files.append(path);f=json.loads(path.read_text())
    records.extend([(v,ROOT/f['archiveDirectory'])for v in f['records']])
assert len(frames)==len(records)==16

def color_at(image, r, x, y):
    return image.getpixel((round((r['x'] + x) * 2), round((r['y'] + y) * 2)))


def region(image, r):
    return [(x - r['x'] * 2, y - r['y'] * 2, image.getpixel((x, y)))
            for y in range(math.ceil(r['y'] * 2), math.floor((r['y'] + r['h']) * 2))
            for x in range(math.ceil(r['x'] * 2), math.floor((r['x'] + r['w']) * 2))]


def ink(image, r, bg):
    points = [(x, y) for x, y, color in region(image, r)
              if max(abs(a - b) for a, b in zip(color, bg)) > 20]
    if not points:
        return None
    return [min(x for x, y in points), min(y for x, y in points),
            max(x for x, y in points), max(y for x, y in points)]


def footprint(image, r, bg):
    # Compare the actual screenshot grid. Subtracting fractional DOM origins
    # invents fractional pixel distances between integer image pixels. Include
    # every intersecting pixel so a clipped final glyph column is not dropped.
    return [(x,y)
            for y in range(math.floor(r['y']*2),math.ceil((r['y']+r['h'])*2))
            for x in range(math.floor(r['x']*2),math.ceil((r['x']+r['w'])*2))
            if max(abs(a-b) for a,b in zip(image.getpixel((x,y)),bg))>20]


def contour_distance(first, second):
    if not first or not second:
        return None
    def directed(points, other):
        buckets={}
        for q in other:
            buckets.setdefault((math.floor(q[0]),math.floor(q[1])),[]).append(q)
        maximum=0
        for x,y in points:
            candidates=[q for dy in range(-3,4) for dx in range(-3,4)
                        for q in buckets.get((math.floor(x)+dx,math.floor(y)+dy),[])]
            nearest=min((max(abs(x-a),abs(y-b)) for a,b in candidates),default=4)
            maximum=max(maximum,nearest)
        return maximum
    return max(directed(first,second),directed(second,first))


def foreground(image, r, bg):
    pixels = [color for x, y, color in region(image, r)
              if max(abs(a - b) for a, b in zip(color, bg)) > 20]
    return Counter(pixels).most_common(1)[0][0] if pixels else None


def compare(name, sample, values, threshold, units):
    delta = max(abs(a - b) for a, b in zip(*values)) if all(v is not None for v in values) else None
    checks.append({'scene': name, 'sample': sample, 'values': values,
                   'maximumDelta': delta, 'units': units, 'threshold': threshold,
                   'passed': delta is not None and delta <= threshold})


for row,archive in records:
    name=row['name'];m={z['id']:z for z in row['styles']};a=frames[name]
    paths=[archive/(name+'.png'),ROOT/'outputs/ui-qa'/(name+'.png')];files.extend(paths);images=[Image.open(path).convert('RGB')for path in paths];assert all(im.size==(2048,1536)for im in images)
    tags=[m['tag'],a['match']];backgrounds=[color_at(im,r,2,11)for im,r in zip(images,tags)]
    compare(name,'tag interior fill',backgrounds,1,'RGB')
    for edge in ['top','bottom','left','right']:
        colors=[]
        for im,r in zip(images,tags):
            x,y={'top':(r['w']/2,.5),'bottom':(r['w']/2,r['h']-.75),'left':(.5,r['h']/2),'right':(r['w']-.75,r['h']/2)}[edge];colors.append(color_at(im,r,x,y))
        compare(name,'tag '+edge+' border interior',colors,1,'RGB')
    for vertical in ['top','bottom']:
        for horizontal in ['left','right']:
            masks=[]
            for im,r,bg in zip(images,tags,backgrounds):
                corner=dict(x=r['x']+(0 if horizontal=='left'else r['w']-6),y=r['y']+(0 if vertical=='top'else r['h']-6),w=6,h=6);masks.append(ink(im,corner,bg))
            compare(name,'tag '+vertical+'-'+horizontal+' corner extent',masks,2,'physical pixels')
    closes=[m['closeIcon'],dict(x=a['match-close']['x']-2,y=a['match-close']['y']-2,w=16,h=16)];labels=[m['content'],a['match-label']]
    components=[('close SVG',closes,backgrounds),('tag text',labels,backgrounds)]
    if m['search']['x']+m['search']['w']<=1024 and 'search'in a:
        searches=[dict(x=r['x']+4,y=r['y']+4,w=r['w']-8,h=r['h']-8)for r in [m['search'],a['search']]];bgs=[color_at(im,r,3,3)for im,r in zip(images,[m['search'],a['search']])];components.append(('search icon',searches,bgs))
    else:skips.append({'scene':name,'component':'search icon','reason':'outside captured viewport; no pixel acceptance'})
    for component,rects,bgs in components:
        compare(name,component+' extent',[ink(im,r,bg)for im,r,bg in zip(images,rects,bgs)],2,'physical pixels')
        compare(name,component+' foreground',[foreground(im,r,bg)for im,r,bg in zip(images,rects,bgs)],1,'RGB')
    contours=[footprint(im,r,bg)for im,r,bg in zip(images,labels,backgrounds)];delta=contour_distance(*contours);checks.append({'scene':name,'sample':'clipped label internal ink footprint','inkPixels':[len(z)for z in contours],'maximumDelta':delta,'units':'physical pixels','coordinateFrame':'absolute PNG pixels','distanceCappedAt':4,'threshold':2,'passed':delta is not None and delta<=2})
    compare(name,'header interior fill',[color_at(im,r,4,3)for im,r in zip(images,[m['header'],a['header']])],1,'RGB')
    divider=m['divider']
    if divider['x']+4<=1024 and 'divider'in a:
        target=a['divider']
        if divider['background']!='rgba(0, 0, 0, 0)':
            for y in [100,350,700]:
                values=[]
                for im,r in zip(images,[divider,target]):
                    values.append([c for x in range(round(r['x']*2),round((r['x']+4)*2))for c in im.getpixel((x,y*2))])
                compare(name,'opaque4px divider row'+str(y),values,1,'RGB')
        else:
            compare(name,'transparent divider left header point',[im.getpixel((round(r['x']*2),88))for im,r in zip(images,[divider,target])],1,'RGB')
    else:skips.append({'scene':name,'component':'divider','reason':'outside captured viewport; no pixel acceptance'})
report={'passed':all(x['passed']for x in checks),'scenes':16,'checks':checks,'skippedOffViewportComponents':skips,'logSHA256':hashlib.sha256(log.read_bytes()).hexdigest(),'files':[{'path':str(path.relative_to(ROOT))if path.is_relative_to(ROOT)else str(path),'sha256':hashlib.sha256(path.read_bytes()).hexdigest()}for path in files],'scope':'Only tag/header points,predefined label/close/search ink extents/foregrounds and opaque4px divider stripes atthree rows. RGB1 and2 physical pixels unchanged. Outside-viewport pixels are explicitly unaccepted. No full bitmap,animation frames,hardware/native or whole-app parity acceptance.'}
(ROOT/args.output).write_text(json.dumps(report,indent=2)+'\n');bad=[x for x in checks if not x['passed']]
for x in bad[:20]:print(x)
assert report['passed'],f'{len(bad)} sidebar bounded paint checks failed'
print('SIDEBAR MACOS IMAGES PASS:',len(checks),'bounded checks in16 states;',len(skips),'outside-viewport components excluded')
