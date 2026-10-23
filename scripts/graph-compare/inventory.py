"""Measurements, tolerances and provenance for the paired graph audit."""
import json
import math
from pathlib import Path
import statistics

KINDS = {'file':'orphan.md', 'tag':'#topic', 'asset':'pixel.png',
         'unresolved':'?Missing Note', 'code':'src/main.py'}
SOURCE = {
    'input':('crates/impro-graph/src/view/input.rs:24','Phase B','Match the measured pointer and keyboard policy.'),
    'camera':('crates/impro-graph/src/view/camera.rs:139','Phase B','Match anchor selection and motion coefficients to the measured camera behaviour.'),
    'draw':('crates/impro-graph/src/view/draw.rs:313','Phase B','Adjust native painting to the measured geometry and opacity policy.'),
    'panel':('crates/impro-graph/src/view/panel.rs:276','Phase B','Match the measured control state and available actions.'),
    'flow':('crates/impro-graph/src/view/flow.rs:100','Phase B','Align the view lifecycle and update timing with the measured host contract.'),
    'data':('crates/impro-graph/src/view/host.rs:1','T2','Trace the differing visible IDs through graph construction before changing the view.'),
    'blank':('crates/impro-graph/src/view/input.rs:315','T1','Keep the mapped view and its controls painted after activation.'),
}


def same(a,b,tolerance):
    if isinstance(a,bool) or isinstance(b,bool):
        return a is b
    if isinstance(a,(int,float)) and isinstance(b,(int,float)):
        return math.isfinite(a) and math.isfinite(b) and abs(a-b)<=tolerance
    if isinstance(a,list) and isinstance(b,list):
        return len(a)==len(b) and all(same(x,y,tolerance) for x,y in zip(a,b))
    if isinstance(a,dict) and isinstance(b,dict):
        return a.keys()==b.keys() and all(same(a[k],b[k],tolerance) for k in a)
    return a==b


def node(s,kind):
    return s['nodes'][KINDS.get(kind,kind)]


def radii(s):
    return {kind:round(node(s,kind)['screen'][2]/math.sqrt(s['state']['zoom']),4)
            for kind in KINDS if KINDS[kind] in s['nodes']}


def pan_delta(s, dest=None):
    end=dest or s['state']
    return [round(a-b,4) for a,b in zip(end['pan'],s['before']['state']['pan'])]


def anchor_error(s):
    b,a=s['before']['state'],s['state']
    return round(math.dist([(p-old)*a['zoom']/b['zoom']+new for p,old,new in
                            zip((200,150),b['pan'],a['pan'])],(200,150)),4)


def spacing(s):
    p=[n['world'] for n in s['nodes'].values()]
    return round(statistics.median(min(math.dist(x,y) for y in p if y is not x) for x in p),2)


def cluster(s):
    c=[v['world'] for k,v in s['nodes'].items() if k.startswith('cluster/')]
    h=[v['world'] for k,v in s['nodes'].items() if k.startswith('hub/')]
    mean=lambda v:[statistics.mean(x[i] for x in v) for i in (0,1)]
    return round(math.dist(mean(c),mean(h)),2)


def retention(s,side):
    deliveries=s['extra']['deliveries']
    if not deliveries:
        raise ValueError('no delivered update was captured')
    if side=='reference':
        last=deliveries[-1]
        before,after=last['before'],last['after']
    else:
        before={k:v['world'] for k,v in s['before']['nodes'].items()}
        after={k:v['world'] for k,v in deliveries[0]['nodes'].items()}
    common=before.keys() & after.keys()
    return round(max((math.dist(before[k],after[k]) for k in common),default=0),5)


def placement(s,side):
    d=s['extra']['deliveries'][-1]
    after=d['after'] if side=='reference' else {k:v['world'] for k,v in d['nodes'].items()}
    return round(math.dist(after['saved.md'],after['hub/Hub.md']),3)


def picture(out,side,scene):
    from PIL import Image
    return Image.open(out/side/(scene+'.png')).convert('RGB')


def hexrgb(color):
    return tuple(int(color[i:i+2],16) for i in (1,3,5))


def centre_samples(out,side,scene,s):
    img=picture(out,side,scene)
    result={}
    for kind,identifier in KINDS.items():
        if identifier not in s['nodes']:
            continue
        x,y,r=s['nodes'][identifier]['screen']
        if not (r+2<x<img.width-r-2 and r+2<y<img.height-r-2):
            continue
        if s['panel_open'] and x>img.width-255 and y<420:
            continue
        # Reject occlusion by any other marker, even when independent layouts differ.
        if any(k!=identifier and math.dist((x,y),v['screen'][:2])<r+v['screen'][2]+3
               for k,v in s['nodes'].items()):
            continue
        result[kind]=list(img.getpixel((round(x),round(y))))
    return result


def pixel_radii(out,side,scene,s):
    img=picture(out,side,scene)
    samples=centre_samples(out,side,scene,s)
    result={}
    for kind,center in samples.items():
        x,y,r=node(s,kind)['screen']
        bg=hexrgb(s['background'])
        threshold=max(8,math.dist(center,bg)*.4)
        widths=[]
        for sign in (-1,1):
            extent=0
            for j in range(1,math.ceil(r*1.5)+5):
                if math.dist(img.getpixel((round(x)+sign*j,round(y))),center)>threshold:
                    break
                extent=j
            widths.append(extent)
        result[kind]=(1+sum(widths))/2
    return result


def line_width(out,side,scene,s,colour=False):
    img=picture(out,side,scene)
    bg=hexrgb(s['background'])
    # Isolated known directed source pair; reject clipping and control overlays.
    a=s['nodes']['src/main.py']['screen']
    b=s['nodes']['src/util.py']['screen']
    x,y=(a[0]+b[0])/2,(a[1]+b[1])/2
    if not (12<x<img.width-12 and 12<y<img.height-12):
        raise ValueError('source-pair midpoint is outside the captured canvas')
    if s['panel_open'] and x>img.width-255 and y<420:
        raise ValueError('source-pair midpoint is covered by controls')
    length=math.dist(a[:2],b[:2]);nx,ny=-(b[1]-a[1])/length,(b[0]-a[0])/length
    for v in s['nodes'].values():
        px,py,r=v['screen'];dx,dy=px-x,py-y
        if abs(dx*ny-dy*nx)<r+1.5 and abs(dx*nx+dy*ny)<10+r:
            raise ValueError('Connection cross-section intersects a marker at this zoom.')
    values=[]
    for j in range(-20,21):
        color=img.getpixel((round(x+nx*j/4),round(y+ny*j/4)))
        values.append(math.dist(color,bg))
    peak=max(values)
    if colour:
        j=values.index(peak)-20
        return list(img.getpixel((round(x+nx*j/4),round(y+ny*j/4))))
    if peak<5:
        raise ValueError('no connection contrast at sampled midpoint')
    # FWHM, 0.25px sampling; allow raster coverage and subpixel orientation tolerance.
    return round(sum(v>=peak/2 for v in values)*.25,2)


def label_pixels(out,side,scene,s):
    img=picture(out,side,scene)
    x,y,r=node(s,'file')['screen']
    z=s['state']['zoom']
    # An isolated file focused at the canvas centre; fixed region excludes the marker.
    box=(round(x-100*math.sqrt(z)), math.ceil(y+r+2),
         round(x+100*math.sqrt(z)), math.ceil(y+r+55*math.sqrt(z)+8))
    if min(box[:2])<0 or box[2]>=img.width or box[3]>=img.height:
        raise ValueError('label sample region is clipped')
    bg=hexrgb(s['background'])
    pixels=[]
    for py in range(box[1],box[3]):
        for px in range(box[0],box[2]):
            if math.dist(img.getpixel((px,py)),bg)>8:
                pixels.append((px,py))
    if not pixels:
        return dict(visible=False,height=0,width=0)
    return dict(visible=True,height=max(p[1] for p in pixels)-min(p[1] for p in pixels)+1,
                width=max(p[0] for p in pixels)-min(p[0] for p in pixels)+1)


def label_alpha(out,side,scene,s):
    img=picture(out,side,scene)
    x,y,r=node(s,'file')['screen']
    z=s['state']['zoom']
    bg=hexrgb(s['background']); fg=hexrgb(s['text_color'])
    direction=[b-a for a,b in zip(bg,fg)]
    denominator=sum(d*d for d in direction)
    estimates=[]
    for py in range(math.ceil(y+r+2),math.ceil(y+r+35*math.sqrt(z)+8)):
        for px in range(round(x-50*math.sqrt(z)),round(x+50*math.sqrt(z))):
            color=img.getpixel((px,py))
            estimates.append(sum((b-a)*d for a,b,d in zip(bg,color,direction))/denominator)
    return round(max(estimates,default=0),3)


def font_changed(out,side,s):
    img=picture(out,side,'font')
    before=picture(out,side,'font-before')
    x,y,r=node(s,'file')['screen']
    box=(round(x-65),math.ceil(y+r+2),round(x+65),math.ceil(y+r+50))
    return img.crop(box).tobytes()!=before.crop(box).tobytes()


def dart_metrics(s):
    result = set()
    for dart in s['paint']['arrows']:
        tip, wing, notch, other = dart['vertices']
        back = [(a+b)/2 for a,b in zip(wing,other)]
        result.add((round(dart['alpha'],3),round(math.dist(tip,back),3),
                    round(math.dist(wing,other)/2,3),round(math.dist(tip,notch),3)))
    return [list(x) for x in sorted(result)]


def hover_lines(s):
    lines = {tuple(sorted(line['endpoints'])):line for line in s['paint']['lines']}
    pairs = [('hub/Hub.md','hub/spoke-0.md'),('src/main.py','src/util.py')]
    try:
        return [lines[tuple(sorted(pair))]['alpha'] for pair in pairs]
    except KeyError as error:
        raise ValueError('Required hover connection was culled from the reference paint.') from error


def repeat_distance(out,side,s):
    path=out/(side+'-repeat')/'measurements.json'
    if not path.exists():
        raise ValueError('Fresh-start repeat omitted by --skip-repeat.')
    nodes=json.loads(path.read_text())['steps']['default']['nodes']
    if nodes.keys()!=s['nodes'].keys():
        raise ValueError('Fresh-start repeat node set changed.')
    return round(max(math.dist(nodes[k]['world'],s['nodes'][k]['world']) for k in nodes),5)


def compact(v):
    s=json.dumps(v,ensure_ascii=False,separators=(',',':'))
    return s if len(s)<145 else s[:142]+'…'


def compare(reference, impro, out):
    rs,is_=reference['steps'],impro['steps']
    if rs.keys()!=is_.keys():
        raise RuntimeError('paired driver step sets differ')
    rows=[]
    def add(identifier,description,scenario,fn,*,tol=0,unit='exact',source='flow',severity=3,reason=None):
        row=dict(id=identifier,behaviour=description,scenario=scenario,tolerance=unit,
                 source=SOURCE[source][0],owner=SOURCE[source][1],fix=SOURCE[source][2],severity=severity)
        if reason:
            row.update(reference=None,impro=None,verdict='unmeasurable',reason=reason)
        else:
            try:
                a=fn(rs[scenario], 'reference')
                b=fn(is_[scenario], 'impro')
                row.update(reference=a,impro=b,verdict='same' if same(a,b,tol) else 'different')
            except (KeyError,ValueError,ImportError) as error:
                row.update(reference=None,impro=None,verdict='unmeasurable',reason=str(error))
        rows.append(row)
    def missing(identifier,description,reason,source='flow'):
        add(identifier,description,'default',lambda *_:None,reason=reason,source=source)
    def window_value(side, name):
        path=out/('window-'+side)/(name+'.json')
        if not path.is_file():
            raise ValueError('Run --window-test-bin with the built test_graph_parity binary for paired full-window evidence.')
        return json.loads(path.read_text())
    add('initial-defaults','Default visibility, force, display, local and camera preferences','default',lambda s,_:s['prefs'],source='panel')
    add('initial-graph','Default visible IDs and undirected line count','default',lambda s,_:[sorted(s['nodes']),s['state']['lines']],source='data',severity=1)
    add('search-label','Search field accessible label and placeholder','default',lambda s,_:s['search'],source='panel')
    add('panel-width','Open controls width','default',lambda s,_:s['panel_width'],tol=1,unit='1 logical px',source='panel')
    add('initial-panel','Controls open by default','default',lambda s,_:s['panel_open'],source='panel')
    add('sections','Section order and default collapsed states','default',lambda s,_:s['sections'],source='panel')
    for key in ('localDepth','textFadeMultiplier','nodeSizeMultiplier','lineSizeMultiplier','centerStrength','repelStrength','linkStrength','linkDistance'):
        add('slider-'+key,'Control label, minimum, maximum, step and default: '+key,'default',lambda s,_,k=key:s['sliders'][k],tol=1e-8,unit='numeric 1e-8; labels exact',source='panel')
    add('toggles','Every toggle label and default checked state','default',lambda s,_:s['toggles'],source='panel')
    add('control-buttons','Add rule, reset, close and replay labels','default',lambda s,_:s['buttons'],source='panel')
    add('section-toggle','Every section toggles its saved collapsed state','sections-toggled',lambda s,_:[s['sections'],{k:v for k,v in s['prefs'].items() if k.startswith('collapse-')}],source='panel')
    add('control-order','Control labels in widget/DOM order','default',lambda s,_:s['labels'],source='panel')
    add('global-local-rows','Global mode hides local hops and direction rows','default',lambda s,_:s['local_rows'],source='panel')
    add('radii-degree','World radii for orphan, hub, spoke, code, tag and missing target','expanded',lambda s,_:{k:round(v['screen'][2]/math.sqrt(s['state']['zoom']),4) for k,v in s['nodes'].items()},tol=.01,unit='0.01 world unit',source='draw')
    for theme in ('dark','light','midnight','ember'):
        for status in ('active','hover'):
            add('colour-'+theme+'-'+status,theme+' '+status+' marker RGB',status+'-'+theme,
                lambda s,_:node(s,'hub/Hub.md')['color'],source='draw')
        for kind in KINDS:
            add('colour-'+theme+'-'+kind,f'{theme} settled {kind} marker RGB','theme-'+theme,lambda s,_,k=kind:node(s,k)['color'],source='draw')
        add('pixel-radius-'+theme,'Screenshot marker radii in logical pixels: '+theme,'theme-'+theme,
            lambda s,side,n='theme-'+theme:pixel_radii(out,side,n,s),tol=1,unit='1 px boundary coverage; shared unoccluded kinds only',source='draw')
        add('pixels-'+theme,'Screenshot marker-centre RGB samples: '+theme,'theme-'+theme,
            lambda s,side,n='theme-'+theme:centre_samples(out,side,n,s),tol=3,unit='3/255 per RGB channel; shared unoccluded kinds only',source='draw')
    for scene in ('active','hover','accent'):
        add('highlight-'+scene,scene+' marker colour','active' if scene=='active' else scene,
            lambda s,_:node(s,'hub/Hub.md')['color'],source='draw')
    for zoom in ('0.1','0.25','0.7','1','2'):
        add('radius-zoom-'+zoom,'Radius scales with square root of zoom: '+zoom,'zoom-'+zoom,
            lambda s,_:radii(s),tol=.01,unit='0.01 world unit after dividing by sqrt(zoom)',source='draw')
        add('line-width-'+zoom,'Screenshot connection width at zoom '+zoom,'zoom-'+zoom,
            lambda s,side,n='zoom-'+zoom:line_width(out,side,n,s),tol=1,unit='1 px FWHM; excludes clipped/covered midpoint',source='draw')
        add('line-colour-'+zoom,'Screenshot connection RGB at zoom '+zoom,'zoom-'+zoom,
            lambda s,side,n='zoom-'+zoom:line_width(out,side,n,s,True),tol=20,unit='20/255 per channel; raster coverage allowance',source='draw')
        add('label-'+zoom,'Screenshot label visibility and ink extent at zoom '+zoom,'label-'+zoom,
            lambda s,side,n='label-'+zoom:label_pixels(out,side,n,s),tol=3,unit='3 px ink bounding box; visibility exact',source='draw')
    for detail in ('-1','2'):
        add('label-detail-'+detail,'Label ink visibility and size with detail '+detail,'detail-'+detail,
            lambda s,side,n='detail-'+detail:label_pixels(out,side,n,s),tol=3,unit='3 px ink bounding box; visibility exact',source='draw')
    add('arrows','Painted arrow opacity, length, half-width and notch at each zoom','zoom-1',
        lambda _,side:{z:dart_metrics((rs if side=='reference' else is_)['zoom-'+z]) for z in ('0.1','0.25','0.7','1','2')},
        tol=.01,unit='0.01 logical px/opacity; culled markers omitted',source='draw')
    for zoom in ('0.1','0.25','0.7','1','2'):
        add('label-alpha-'+zoom,'Peak screenshot label contrast versus text role at zoom '+zoom,'label-'+zoom,
            lambda s,side,n='label-'+zoom:label_alpha(out,side,n,s),tol=.15,unit='0.15 alpha estimate; font coverage differs',source='draw')
    add('label-font-size','Nominal label font size and wrapping width','label-1',
        lambda s,_:[s['paint']['labels']['orphan.md'][k] for k in ('font_size','screen_font_size','wrap_width')],
        tol=.01,unit='0.01 logical px; requested sizes before glyph coverage',source='draw')
    for zoom in ('0.25','0.7','1','2'):
        add('label-nominal-alpha-'+zoom,'Effective label alpha before glyph coverage: '+zoom,'label-'+zoom,
            lambda s,_:s['paint']['labels']['orphan.md']['alpha'],tol=.001,unit='0.001 opacity',source='draw')
    add('hover-dim','Hover leaves unrelated markers at 28 percent opacity','hover',lambda s,_:[node(s,k)['alpha'] for k in ('orphan.md','src/main.py','hub/spoke-0.md','hub/Hub.md')],tol=.01,unit='0.01 opacity',source='draw',severity=2)
    add('cursor','Hover installs a pointer cursor','hover',lambda s,side:s['cursor']=='pointer' if side=='reference' else s['cursor_set'],source='input')
    add('hover-edges','Hover connection alpha after paint bucketing','hover',lambda s,_:hover_lines(s),tol=.001,unit='0.001 opacity; touching/unrelated connections',source='draw')
    add('cursor-shape','Exact grabbing and pointer cursor names','hover',
        lambda s,side:[s['cursor'],(rs if side=='reference' else is_)['controlled-motion']['extra']['grabbing_cursor']],source='input')
    for scene in ('click','ctrl-click','middle-click','slop-6','slop-7','tag-click','missing-click','tag-right','missing-right','key-enter'):
        add('events-'+scene,'Emitted graph actions: '+scene,scene,lambda s,_:s['events'],source='input',severity=1)
    add('menu-actions','Each file menu item emits its matching host action','menu-actions',lambda s,_:s['events'],source='input')
    add('file-menu','Right-click file menu items and order','file-menu',lambda s,_:s['extra']['menu'],source='panel')
    add('drag-held','Node drag is pinned to pointer and layout stays warm','drag',lambda s,_:[s['extra']['held']['state']['dragging'],s['extra']['held']['state']['simulating'],round(s['extra']['held']['state']['alpha'],2)],tol=.03,unit='0.03 layout alpha; flags exact',source='input')
    add('drag-follow','Held marker stays at the requested pointer displacement during simulation','drag',lambda s,_:[round((a-b)*s['extra']['held']['state']['zoom'],3) for a,b in zip(node(s['extra']['held'],'hub/Hub.md')['world'],node(s['before'],'hub/Hub.md')['world'])],tol=1,unit='1 screen px',source='input')
    add('drag-release','Releasing a drag emits no open action and settles','drag',lambda s,_:[s['events'],s['state']['dragging'],s['state']['simulating']],source='input')
    add('pan-distance','Background drag follows 80 by 32 screen pixels','pan',lambda s,_:pan_delta(s,s['extra']['held']),tol=.1,unit='0.1 px',source='camera')
    add('pan-inertia','Inertia travel after the scripted 8-move fling','pan',lambda s,_:[round(a-b,2) for a,b in zip(s['state']['pan'],s['extra']['held']['pan'])],tol=8,unit='8 px scheduling allowance',source='camera',severity=2)
    add('pan-stop','Pressing the background stops inertia immediately','stop-on-press',lambda s,_:round(math.dist(s['extra']['pressed']['pan'],s['extra']['later']['pan']),3),tol=.05,unit='0.05 px',source='camera')
    add('pan-decay','Pan velocity decay over a controlled 60 Hz frame','controlled-motion',lambda s,_:s['extra']['pan_decay'],tol=1e-6,unit='1e-6 retention factor',source='camera')
    add('pan-filter','Released velocity after eight controlled 16 ms pointer moves','controlled-motion',lambda s,_:s['extra']['fling_velocity'],tol=1e-6,unit='1e-6 logical px/ms',source='camera')
    for scene in ('wheel-in','wheel-out','ctrl-wheel','wheel-max','wheel-min'):
        add('zoom-'+scene,'Wheel target factor or limit: '+scene,scene,lambda s,_:round(s['state']['zoom']/s['before']['state']['zoom'],7),tol=1e-6,unit='1e-6 zoom ratio',source='camera')
    for scene in ('wheel-in','wheel-out'):
        add('anchor-'+scene,'Distance the pointer anchor moves: '+scene,scene,lambda s,_:anchor_error(s),tol=.1,unit='0.1 px',source='camera',severity=2)
    add('reduced-motion','Live reduced-motion preference reaches the zoom target within 40 ms','reduced-wheel',lambda s,_:abs(s['extra']['at40']['zoom']-s['extra']['at40']['target_zoom'])<1e-6,source='camera')
    add('zoom-ease','Wheel zoom easing over a controlled 60 Hz frame','controlled-motion',lambda s,_:s['extra']['zoom_ease'],tol=1e-6,unit='1e-6 fraction of remaining zoom',source='camera')
    for key in ('ArrowLeft','ArrowRight','ArrowUp','ArrowDown','+','-'):
        add('keyboard-'+key,'Held 200 ms key movement: '+key,'key-'+key,
            lambda s,_:pan_delta(s) if key.startswith('Arrow') else round(s['state']['zoom']/s['before']['state']['zoom'],4),tol=25 if key.startswith('Arrow') else .12,
            unit='25 px pan or 0.12 zoom ratio scheduling allowance',source='camera')
    add('keyboard-shift','Shift arrow travel relative to the unmodified arrow','key-shift',lambda s,side:round(pan_delta(s)[0]/pan_delta((rs if side=='reference' else is_)['key-ArrowLeft'])[0],3),tol=.35,unit='0.35 travel ratio; 200 ms held keys',source='input')
    for scene in ('key-escape','key-escape-again'):
        add(scene,'Escape closes controls, then clears hover: '+scene,scene,lambda s,_:[s['panel_open'],s['state']['hover']],source='input')
    add('camera-reset','Reset camera recentres, restores zoom one and keeps the graph','camera-reset',lambda s,_:[s['state']['zoom'],s['state']['pan'],s['state']['nodes']],tol=.1,unit='0.1 logical px; zoom exact',source='camera')
    missing('fit-camera','Fit camera command','No reference fit-camera entry point exists in the graph widget or window. The native method is an extra feature without a paired oracle command.','camera')
    add('layout-settle','Fresh layout wall time to settle','default',lambda _,side:round((reference if side=='reference' else impro)['initial_settle_ms']),tol=1500,unit='1500 ms scheduling allowance',source='flow')
    add('layout-spacing','Settled median nearest-neighbour spacing','default',lambda s,_:spacing(s),tol=20,unit='20 world units; independent layouts',source='draw')
    add('layout-clusters','Isolated-cluster to hub centroid separation','default',lambda s,_:cluster(s),tol=80,unit='80 world units; independent layouts',source='draw')
    add('layout-determinism','Maximum fresh-start world-position difference against a second process','default',lambda s,side:repeat_distance(out,side,s),tol=.05,unit='0.05 world unit in each implementation',source='flow')
    add('search','Live search produces the filtered IDs','search',lambda s,_:sorted(s['nodes']),source='data',severity=2)
    add('local','Local depth two, both directions, visible IDs and rows','local',lambda s,_:[sorted(s['nodes']),s['local_rows'],s['prefs']['localDepth']],source='data',severity=2)
    add('local-outgoing','Outgoing-only traversal IDs','local-outgoing',lambda s,_:sorted(s['nodes']),source='data')
    add('note-mode','Note graph visible IDs','note',lambda s,_:sorted(s['nodes']),source='data')
    def menu_value(s,_):
        if s['extra'].get('menu') is None:
            raise ValueError('Native popup not exposed in GTK toplevel traversal; request a type-menu inspection helper.')
        return s['extra']['menu']
    add('type-menu','Global / Local / Note graph menu labels and order','type-menu',menu_value,source='panel')
    add('closed-chip','Closing the controls exposes the three-button chip','key-escape',lambda s,_:s['chip_visible'],source='panel')
    for scene in ('group-add','group-remove'):
        add(scene,'Colour rule count and generated colour: '+scene,scene,lambda s,_:s['prefs']['colorGroups'],source='panel')
    add('group-colour','First matching colour rule colours file markers','group-colour',lambda s,_:[node(s,'hub/Hub.md')['color'],node(s,'tag')['color']],source='draw')
    add('reset','Reset restores defaults while retaining search, zoom and section visibility','reset',lambda s,_:s['prefs'],tol=1e-8,unit='1e-8 numeric; other fields exact',source='panel')
    for scene in ('replay','replay-reduced'):
        add(scene,'Replay visible IDs at 250 ms and 1000 ms: '+scene,scene,
            lambda s,_:[sorted(s['extra'][t]['nodes']) for t in ('at250','at1000')],source='flow',severity=2)
    for scene in ('replay','replay-reduced'):
        add('duration-'+scene,'Replay reveal plus final cooling wall time: '+scene,scene,lambda s,_:round(s['elapsed_ms']),tol=1500,unit='1500 ms scheduling allowance',source='flow')
    add('replay-speed','Actual reveal-plan chronology, item boundaries and reveal rate','replay',lambda s,_:s['extra']['plan'],tol=1e-8,unit='item indices and IDs exact; rate 1e-8 items/s',source='flow')
    add('status-ready','Status is hidden after a nonempty graph arrives','default',lambda s,_:s['status'],source='flow')
    add('status-empty','Empty-filter status text and zero nodes','empty',lambda s,_:[s['status'],s['state']['nodes']],source='flow',severity=2)
    add('status-local','No-active-file local status and zero nodes','local-empty',lambda s,_:[s['status'],s['state']['nodes']],source='flow',severity=2)
    add('status-loading','Initial Mapping this project… status before data delivery','default',
        lambda _,side:(reference if side=='reference' else impro)['loading_status'],source='flow')
    add('truncation','Truncation cap count and notice text','cap',lambda s,_:[s['state']['nodes'],s['notice']],source='data')
    for scene in ('save','rename','delete'):
        add('live-'+scene,'Live '+scene+' updates visible IDs',scene,lambda s,_:sorted(s['nodes']),source='data',severity=2)
        add('retention-'+scene,'Survivor world-coordinate displacement at update delivery: '+scene,scene,
            lambda s,side:retention(s,side),tol=.05,unit='0.05 world unit at delivery',source='flow',severity=2)
    add('new-placement','New file distance from its surviving hub neighbour at delivery','save',lambda s,side:placement(s,side),tol=34,unit='34 world units; independent seeds within a 24-unit jitter square',source='flow')
    add('buffer-200','Unsaved-buffer visible IDs after 200 ms','buffer',lambda s,_:sorted(s['extra']['at200']['nodes']),source='flow',severity=2)
    add('buffer-600','Unsaved-buffer visible IDs after 600 ms','buffer',lambda s,_:sorted(s['extra']['at600']['nodes']),source='data')
    add('buffer-host-debounce','Full-window edit debounce and local graph following tab changes','default',
        lambda _,side:window_value(side,'buffer'),tol=150,unit='150 ms timer dispatch allowance; flags exact',source='flow')
    add('font-live','Changing font family changes the focused label pixels','font',lambda s,side:font_changed(out,side,s),source='draw')
    add('resize','Dock-sized widget resize preserves centre and mapped graph','resize',lambda s,_:[s['state']['size'],pan_delta(s),s['mapped']],tol=.1,unit='0.1 logical px',source='camera')
    add('hidden','No frames advance during one second while unmapped','hidden',lambda s,_:s['extra']['at1000'].get('frames',s['extra']['at1000'].get('ticks',0))-s['extra']['at0'].get('frames',s['extra']['at0'].get('ticks',0)),source='flow',severity=1)
    add('shown','Showing the hidden widget restores a nonempty mapped graph','shown',lambda s,_:[s['mapped'],s['state']['nodes'],s['panel_open']],source='blank',severity=1)
    add('idle-cpu','Two-second settled CPU ticks (100 ticks per second on this machine)','idle',lambda s,_:s['extra']['cpu_ticks'],tol=2,unit='2 ticks; page descendants included on reference side',source='flow')
    add('persistence','Persisted blob restores search, panel, zoom and all graph preferences','persist',lambda s,_:s['extra']['restored'],tol=1e-8,unit='1e-8 numeric; strings/flags exact',source='panel')
    add('persistence-restart','Real preferences restored by a fresh full-window process','default',
        lambda _,side:window_value(side,'settings-restore'),tol=1e-8,unit='1e-8 zoom; flags exact',source='flow')
    add('persistence-delay','Real preference and zoom writes after their debounce delays','default',
        lambda _,side:window_value(side,'settings-save'),tol=150,unit='150 ms dispatch/write allowance; early-write checks exact',source='flow')
    add('pan-visible','Count of on-screen markers after the background drag','pan',lambda s,_:sum(0<=v['screen'][0]<s['state']['size'][0] and 0<=v['screen'][1]<s['state']['size'][1] for v in s['nodes'].values()),tol=8,unit='8 markers; independently placed scenes',source='blank',severity=1)
    add('click-visible','Graph remains nonempty and mapped after pointer/keyboard actions','key-escape-again',lambda s,_:[s['mapped'],s['state']['nodes']>0],source='blank',severity=1)
    for row in rows:
        if row['id']=='duration-replay-reduced':
            row.update(source='crates/impro-graph/src/view/flow.rs:503',severity=4,
                       fix='Re-measure completion after T2 aligns chronology, then check whether unchanged replay topology unnecessarily reheats the layout.')
        if row['id']=='idle-cpu' and row['verdict']=='different' and row['impro']<row['reference']:
            row.update(source='crates/impro-graph/src/view/flow.rs:952',severity=4,
                       fix='Retain the lower native idle work and repeat a longer sample before attributing remaining page-process CPU activity.')
        if row['id']=='closed-chip':
            row.update(source='crates/impro-graph/src/view/panel.rs:538',owner='T1',severity=1,
                       fix='Explicitly show the guarded chip and its children when closing controls; show_all honours no-show-all and leaves the strip hidden.')
        if row['id']=='label-alpha-0.7':
            row.update(source='crates/impro-graph/src/view/draw.rs:164',
                       fix='Measure nominal label alpha separately from font-mask coverage before adjusting native label fade.')
        if row['id']=='label-1':
            row.update(source='crates/impro-graph/src/view/draw.rs:164',
                       fix='Check Pango font sizing and label bitmap insets against the measured orphan ink bounds at zoom one.')
        if row['id']=='buffer-200':
            row.update(source='crates/impro-graph/src/view/flow.rs:301',severity=4,
                       fix='Compare the full editor forwarding path before changing debounce; keep the 400 ms policy until a window trace establishes a user-visible difference.')
        if row['id']=='anchor-wheel-out':
            row.update(source='crates/impro-graph/src/view/input.rs:209',
                       fix='Use the canvas centre as the anchor when the wheel reduces zoom.')
        if row['id']=='layout-clusters':
            row.update(source='crates/impro-graph/src/view/scene.rs:269',
                       fix='Check cluster-separation distributions on a second fixture before tuning native spacing, since exact independent layout coordinates are not required.')
        if row['id'] in ('replay','replay-reduced'):
            row.update(source='crates/impro-graph/src/view/timelapse.rs:25',owner='T2',
                       fix='Align the index timestamp fallback with the reference before sorting replay chronology.')
        if row['id'] in ('pan-distance','pan-inertia','pan-visible'):
            row.update(source='crates/impro-graph/src/view/scene.rs:724',owner='T1',severity=1,
                       fix='Pass the background-press state into camera advancement so drag velocity is not integrated as released inertia.')
    # Screenshot centre comparisons must compare only the shared unoccluded kind samples.
    for row in rows:
        if row['id'].startswith(('pixels-','pixel-radius-')) and row['verdict']!='unmeasurable':
            common=row['reference'].keys() & row['impro'].keys()
            if not common:
                row.update(verdict='unmeasurable',reason='No shared unoccluded marker centre lies inside both frames.')
            else:
                row['reference']={k:row['reference'][k] for k in sorted(common)}
                row['impro']={k:row['impro'][k] for k in sorted(common)}
                row['verdict']='same' if same(row['reference'],row['impro'],1 if row['id'].startswith('pixel-radius-') else 3) else 'different'
    (out/'comparison.json').write_text(json.dumps(rows,indent=2,ensure_ascii=False)+'\n')
    header='behaviour | reference value | impro value | verdict'
    table=[header,'--- | --- | --- | ---']
    for row in rows:
        table.append(' | '.join((row['id'],compact(row['reference']),compact(row['impro']),row['verdict'])))
    print('\n'.join(table))
    (out/'comparison.md').write_text('\n'.join(table)+'\n')
    counts={v:sum(r['verdict']==v for r in rows) for v in ('same','different','unmeasurable')}
    print('Counts:',json.dumps(counts))
    (out/'counts.json').write_text(json.dumps(counts,indent=2)+'\n')
    write_review_docs(rows,out,counts)


def write_review_docs(rows,out,counts):
    """Emit review-ready documentation under the artifact directory only."""
    from datetime import datetime
    from zoneinfo import ZoneInfo
    stamp=datetime.now(ZoneInfo('Asia/Kolkata')).strftime('%Y-%m-%d %H:%M IST')
    register=['<!-- audit-register -->','',f'Measured {stamp}.',
              '',f"{len(rows)} rows: {counts['same']} same, {counts['different']} different, "
              f"{counts['unmeasurable']} unmeasurable.",'',
              '| ID | Observable behaviour | Scenario | Verdict | Tolerance or coverage reason |',
              '| --- | --- | --- | --- | --- |']
    for r in rows:
        detail=r.get('reason',r['tolerance']).replace('|','\\|').replace('\n',' ')
        register.append(f"| {r['id']} | {r['behaviour']} | {r['scenario']} | {r['verdict']} | {detail} |")
    (out/'behaviour-register.md').write_text('\n'.join(register)+'\n')
    priorities={'pan-visible':0,'pan-distance':1,'pan-inertia':2,'closed-chip':3}
    differences=sorted((r for r in rows if r['verdict']=='different'),key=lambda r:(r['severity'],priorities.get(r['id'],100),r['id']))
    doc=['# Graph audit divergences','',f'Audit completed {stamp}.',
         '',f"{len(rows)} observable rows: {counts['same']} same, {counts['different']} different, "
         f"{counts['unmeasurable']} unmeasurable. Every measured difference is listed below. "
         'Rows sharing a scenario can describe the same root cause. Missing telemetry is a coverage gap, not parity.',
         '', 'Reproduce with `bash scripts/graph-compare/run.sh` after setting the reference and Cairo-bridge '
         'environment variables described in README. The runner owns Xvfb, D-Bus, the shared lock and memory scope. '
         'Divergences do not produce a failing exit status.',
         '', 'Full values and tolerances: `.ceo/graph-compare/comparison.json`. Each driver writes '
         '`.ceo/graph-compare/{reference,impro}/measurements.json`; the JSON key is `steps.<scenario>`. '
         'PNG evidence is included only where captured. The reference checkout is read only.', '']
    for number,r in enumerate(differences,1):
        doc.extend([f"## {number}. {r['behaviour']}",'',
                    f"Owner: **{r['owner']}**. Scenario: `{r['scenario']}`. Row: `{r['id']}`.",'',
                    'Reference: `'+json.dumps(r['reference'],ensure_ascii=False,separators=(',',':'))+'`.',
                    '', 'impro: `'+json.dumps(r['impro'],ensure_ascii=False,separators=(',',':'))+'`.',
                    '',f"Tolerance: {r['tolerance']}.",'',
                    f"Suspected location: `{r['source']}`. Proposed fix: {r['fix']}",'',
                    f"Evidence: run the command above; inspect `steps.{r['scenario']}` in both measurement files "
                    f"and row `{r['id']}` in `.ceo/graph-compare/comparison.json`."])
        pngs=[f'`.ceo/graph-compare/{side}/{r["scenario"]}.png`' for side in ('reference','impro')
              if (out/side/(r['scenario']+'.png')).exists()]
        if pngs:
            doc.extend(['','Frames: '+', '.join(pngs)+'.'])
        if r['id']=='closed-chip':
            doc.extend(['','An isolated GTK probe confirms the visibility mechanism: a box guarded by `no-show-all` stays hidden after `show_all`, and becomes visible after explicit `show`. The native chip has that guard at `panel.rs:538`, while both close paths call only `show_all` at lines 701 and 869. Probe output: `.ceo/graph-compare/gtk-visibility.txt`. The runtime graph frames independently show the missing strip.'])
        if r['id'].startswith(('label-1','label-alpha-')):
            doc.extend(['','This is a raster observation, not a nominal font/alpha mismatch proven by state telemetry. Font-mask coverage, cached bitmap scaling and glyph padding can affect these samples. Request the paint telemetry listed below before adjusting style constants.'])
        if r['id'] in ('replay','replay-reduced'):
            doc.extend(['','Read-only source inspection explains the likely data cause: on this Linux filesystem the '
                        'reference uses the minimum of change/modified timestamps when its runtime does not expose birth time; '
                        'the native index prefers the filesystem creation timestamp. The fixture has equal old modified times, '
                        'so the reference breaks ties by ID while impro starts with the first-created hub. '
                        'The initial snapshot records reference node chronology and native filesystem birth-time evidence. '
                        'Inspect `crates/impro-graph/src/index/scan.rs:163` together with the view sorting site.'])
        if r['id']=='duration-replay-reduced':
            doc.extend(['','Treat this as a timing observation requiring repetition. The earlier complete pair in `.ceo/graph-compare-reviewed` completed this step in approximately 13.8 seconds on the reference and 13.6 seconds on impro. The final reference sample is shorter. Chronology and the time of the last structural change affect cooling, so align the T2 data difference before changing replay pacing.'])
        if r['id']=='buffer-200':
            doc.extend(['','This is a widget-boundary diagnostic. The reference receives direct `set_text` calls; '
                        'the native widget itself debounces buffer updates. The reference application can debounce '
                        'before calling its widget. This row does not establish an application-level delay defect.'])
        doc.append('')
    doc.extend(['## Blank, crash and deadline observations','',
                'The frame-paced background drag is the blank-canvas candidate: start from a settled graph at zoom 1, '
                'press empty canvas at (2,2), send eight pointer moves by (10,4) px with a 16 ms main-loop interval, '
                'then release at (82,34). Compare the held pan and the visible marker count. The native probe uses '
                'GTK-dispatched GDK events with monotonic timestamps. The matching reference receives DOM pointer events. '
                'The controls remain present; this does not reproduce disappearance of the entire dock body.',
                '', 'No process crash or full-widget disappearance was observed in the completed scripted run. '
                'An initial exploratory run reached replay and hit an overly short 12-second harness settle deadline; '
                'the completed run allows 30 seconds for reveal plus cooling. The original deadline log is '
                '`.ceo/graph-compare-initial/impro-driver.log`. This was not evidence of a product hang.',
                '', '## Remaining measurements and Phase B requests','',
                'The complete unmeasurable register is in BEHAVIOURS.md and the JSON table. Needed view helpers are '
                'read-only line/arrow/label paint telemetry, effective nominal font size/wrap/alpha, cursor names, '
                'camera velocity and controlled frame stepping. No hook was added inside the production view.',
                '', 'Full-window work is still needed for actual editor tab/dock routing, the editor-to-graph 400 ms '
                'debounce boundary, persisted settings through a fresh application process, and write timing. '
                'The transient loading state was not captured before data delivery. Wayland and the user desktop were '
                'not accessed. Exact arrow opacity and font-wrap metrics remain unmeasured; raster bounds and sampled '
                'connection widths/colours are included where unobscured.'])
    repo=Path(__file__).resolve().parents[2]
    prefix=str(out.relative_to(repo)) if out.is_relative_to(repo) else str(out)
    (out/'divergences.md').write_text(('\n'.join(doc)+'\n').replace('.ceo/graph-compare/',prefix+'/'))
