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
        if any(k!=identifier and math.dist((x,y),v['screen'][:2])<v['screen'][2]+3
               for k,v in s['nodes'].items()):
            continue
        result[kind]=list(img.getpixel((round(x),round(y))))
    return result


def line_width(out,side,scene,s):
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
    values=[]
    for j in range(-20,21):
        color=img.getpixel((round(x+nx*j/4),round(y+ny*j/4)))
        values.append(math.dist(color,bg))
    peak=max(values)
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
    add('initial-defaults','Default visibility, force, display, local and camera preferences','default',lambda s,_:s['prefs'],source='panel')
    add('initial-graph','Default visible IDs and undirected line count','default',lambda s,_:[sorted(s['nodes']),s['state']['lines']],source='data',severity=1)
    add('initial-panel','Controls open by default','default',lambda s,_:s['panel_open'],source='panel')
    add('sections','Section order and default collapsed states','default',lambda s,_:s['sections'],source='panel')
    for key in ('localDepth','textFadeMultiplier','nodeSizeMultiplier','lineSizeMultiplier','centerStrength','repelStrength','linkStrength','linkDistance'):
        add('slider-'+key,'Control label, minimum, maximum, step and default: '+key,'default',lambda s,_,k=key:s['sliders'][k],tol=1e-8,unit='numeric 1e-8; labels exact',source='panel')
    add('toggles','Every toggle label and default checked state','default',lambda s,_:s['toggles'],source='panel')
    add('control-order','Control labels in widget/DOM order','default',lambda s,_:s['labels'],source='panel')
    add('global-local-rows','Global mode hides local hops and direction rows','default',lambda s,_:s['local_rows'],source='panel')
    add('radii-degree','World radii for orphan, hub, spoke, code, tag and missing target','expanded',lambda s,_:{k:round(v['screen'][2]/math.sqrt(s['state']['zoom']),4) for k,v in s['nodes'].items()},tol=.01,unit='0.01 world unit',source='draw')
    for theme in ('dark','light','midnight','ember'):
        for kind in KINDS:
            add('colour-'+theme+'-'+kind,f'{theme} settled {kind} marker RGB','theme-'+theme,lambda s,_,k=kind:node(s,k)['color'],source='draw')
        add('pixels-'+theme,'Screenshot marker-centre RGB samples: '+theme,'theme-'+theme,
            lambda s,side,n='theme-'+theme:centre_samples(out,side,n,s),tol=3,unit='3/255 per RGB channel; shared unoccluded kinds only',source='draw')
    for scene in ('active','hover','accent'):
        add('highlight-'+scene,scene+' marker colour','active' if scene=='active' else scene,
            lambda s,_:node(s,'hub/Hub.md')['color'],source='draw')
    for zoom in ('0.1','0.25','0.7','1','2'):
        add('radius-zoom-'+zoom,'Radius scales with square root of zoom: '+zoom,'zoom-'+zoom,
            lambda s,_:radii(s),tol=.01,unit='0.01 world unit after dividing by sqrt(zoom)',source='draw')
        add('line-width-'+zoom,'Screenshot connection width at zoom '+zoom,'zoom-'+zoom,
            lambda s,side,n='zoom-'+zoom:line_width(out,side,n,s),tol=.75,unit='0.75 px FWHM; excludes clipped/covered midpoint',source='draw')
        add('label-'+zoom,'Screenshot label visibility and ink extent at zoom '+zoom,'label-'+zoom,
            lambda s,side,n='label-'+zoom:label_pixels(out,side,n,s),tol=3,unit='3 px ink bounding box; visibility exact',source='draw')
    for detail in ('-1','2'):
        add('label-detail-'+detail,'Label ink visibility and size with detail '+detail,'detail-'+detail,
            lambda s,side,n='detail-'+detail:label_pixels(out,side,n,s),tol=3,unit='3 px ink bounding box; visibility exact',source='draw')
    missing('arrows','Arrow visibility, opacity and dart length at each zoom',
            'Native view exposes no arrow geometry or paint telemetry; screenshot lines and labels cannot isolate arrow alpha reliably. Request a read-only painted-arrow helper.','draw')
    for zoom in ('0.1','0.25','0.7','1','2'):
        add('label-alpha-'+zoom,'Peak screenshot label contrast versus text role at zoom '+zoom,'label-'+zoom,
            lambda s,side,n='label-'+zoom:label_alpha(out,side,n,s),tol=.15,unit='0.15 alpha estimate; font coverage differs',source='draw')
    missing('label-font-size','Nominal label font size and wrapping width',
            'Ink bounds and visibility are measured separately; the native label cache exposes neither effective font size nor wrap width. Request label paint telemetry.','draw')
    add('hover-dim','Hover leaves unrelated markers at 28 percent opacity','hover',lambda s,_:[node(s,k)['alpha'] for k in ('orphan.md','src/main.py','hub/spoke-0.md','hub/Hub.md')],tol=.01,unit='0.01 opacity',source='draw',severity=2)
    missing('cursor','Pointer and grabbing cursors','Reference CSS cursor is captured; native GDK cursor has no portable name getter through the allowed helpers. Request a cursor-name QA helper.','input')
    for scene in ('click','ctrl-click','middle-click','slop-6','slop-7','tag-click','missing-click','tag-right','missing-right','key-enter'):
        add('events-'+scene,'Emitted graph actions: '+scene,scene,lambda s,_:s['events'],source='input',severity=1)
    add('file-menu','Right-click file menu items and order','file-menu',lambda s,_:s['extra']['menu'],source='panel')
    add('drag-held','Node drag is pinned to pointer and layout stays warm','drag',lambda s,_:[s['extra']['held']['state']['dragging'],s['extra']['held']['state']['simulating'],round(s['extra']['held']['state']['alpha'],2)],tol=.03,unit='0.03 layout alpha; flags exact',source='input')
    add('drag-release','Releasing a drag emits no open action and settles','drag',lambda s,_:[s['events'],s['state']['dragging'],s['state']['simulating']],source='input')
    add('pan-distance','Background drag follows 80 by 32 screen pixels','pan',lambda s,_:pan_delta(s,s['extra']['held']),tol=.1,unit='0.1 px',source='camera')
    add('pan-inertia','Inertia travel after the scripted 8-move fling','pan',lambda s,_:[round(a-b,2) for a,b in zip(s['state']['pan'],s['extra']['held']['pan'])],tol=8,unit='8 px scheduling allowance',source='camera',severity=2)
    add('pan-stop','Pressing the background stops inertia immediately','stop-on-press',lambda s,_:round(math.dist(s['extra']['pressed']['pan'],s['extra']['later']['pan']),3),tol=.05,unit='0.05 px',source='camera')
    missing('pan-decay','Frame-independent pan velocity decay coefficient','Travel is measured; the native testing API exposes no velocity or timestamped motion input, so exact decay cannot be separated from its synthetic timestamp fallback.','camera')
    for scene in ('wheel-in','wheel-out','ctrl-wheel','wheel-max','wheel-min'):
        add('zoom-'+scene,'Wheel target factor or limit: '+scene,scene,lambda s,_:round(s['state']['zoom']/s['before']['state']['zoom'],7),tol=1e-6,unit='1e-6 zoom ratio',source='camera')
    for scene in ('wheel-in','wheel-out'):
        add('anchor-'+scene,'Distance the pointer anchor moves: '+scene,scene,lambda s,_:anchor_error(s),tol=.1,unit='0.1 px',source='camera',severity=2)
    missing('zoom-ease','Per-frame wheel zoom easing','Native immediate state is synchronous; the page round trip can paint before introspection. A controlled frame/time hook is required for matched ease samples.','camera')
    for key in ('ArrowLeft','ArrowRight','ArrowUp','ArrowDown','+','-'):
        add('keyboard-'+key,'Held 200 ms key movement: '+key,'key-'+key,
            lambda s,_:pan_delta(s) if key.startswith('Arrow') else round(s['state']['zoom']/s['before']['state']['zoom'],4),tol=20 if key.startswith('Arrow') else .12,
            unit='20 px pan or 0.12 zoom ratio scheduling allowance',source='camera')
    missing('keyboard-shift','Shift accelerates arrow and zoom keys','Existing native key helper sends no modifier state and resets the Shift flag on the following arrow press; its shift trace is retained but cannot establish real-key parity. Request a modifier-aware helper.','input')
    for scene in ('key-escape','key-escape-again'):
        add(scene,'Escape closes controls, then clears hover: '+scene,scene,lambda s,_:[s['panel_open'],s['state']['hover']],source='input')
    missing('fit-reset','Fit and reset camera commands','The reference graph page and widget have no fit/reset command; host command-palette routing is outside this widget harness. Native public methods alone are insufficient for a paired observation.','camera')
    add('layout-settle','Fresh layout wall time to settle','default',lambda _,side:round((reference if side=='reference' else impro)['initial_settle_ms']),tol=1500,unit='1500 ms scheduling allowance',source='flow')
    add('layout-spacing','Settled median nearest-neighbour spacing','default',lambda s,_:spacing(s),tol=20,unit='20 world units; independent layouts',source='draw')
    add('layout-clusters','Isolated-cluster to hub centroid separation','default',lambda s,_:cluster(s),tol=80,unit='80 world units; independent layouts',source='draw')
    add('layout-determinism','Maximum fresh-start world-position difference against a second process','default',lambda s,side:repeat_distance(out,side,s),tol=.05,unit='0.05 world unit in each implementation',source='flow')
    add('search','Live search produces the filtered IDs','search',lambda s,_:sorted(s['nodes']),source='data',severity=2)
    add('local','Local depth two, both directions, visible IDs and rows','local',lambda s,_:[sorted(s['nodes']),s['local_rows'],s['prefs']['localDepth']],source='data',severity=2)
    add('local-outgoing','Outgoing-only traversal IDs','local-outgoing',lambda s,_:sorted(s['nodes']),source='data')
    add('note-mode','Note graph visible IDs','note',lambda s,_:sorted(s['nodes']),source='data')
    missing('type-menu','Global / Local / Note graph menu labels and chip','Mode changes are measured through public APIs; native panel exposes neither type menu nor chip button handles. Request a type-menu inspection helper.','panel')
    for scene in ('group-add','group-remove'):
        add(scene,'Colour rule count and generated colour: '+scene,scene,lambda s,_:s['prefs']['colorGroups'],source='panel')
    add('group-colour','First matching colour rule colours file markers','group-colour',lambda s,_:[node(s,'hub/Hub.md')['color'],node(s,'tag')['color']],source='draw')
    add('reset','Reset restores defaults while retaining search, zoom and section visibility','reset',lambda s,_:s['prefs'],tol=1e-8,unit='1e-8 numeric; other fields exact',source='panel')
    for scene in ('replay','replay-reduced'):
        add(scene,'Replay visible IDs at 250 ms and 1000 ms: '+scene,scene,
            lambda s,_:[sorted(s['extra'][t]['nodes']) for t in ('at250','at1000')],source='flow',severity=2)
    missing('replay-speed','Exact chronological replay order and reveal rate','Coarse replay snapshots are measured; creation timestamps and asynchronous timer dispatch prevent an exact items/second comparison without reveal-event telemetry.','flow')
    add('status-ready','Status is hidden after a nonempty graph arrives','default',lambda s,_:s['status'],source='flow')
    add('status-empty','Empty-filter status text and zero nodes','empty',lambda s,_:[s['status'],s['state']['nodes']],source='flow',severity=2)
    add('status-local','No-active-file local status and zero nodes','local-empty',lambda s,_:[s['status'],s['state']['nodes']],source='flow',severity=2)
    missing('status-loading','Initial Mapping this project… status','Both widgets receive data before the first settled snapshot; a blocked-index fixture or pre-data screenshot is needed to measure this transient.','flow')
    add('truncation','Truncation cap count and notice text','cap',lambda s,_:[s['state']['nodes'],s['notice']],source='data')
    for scene in ('save','rename','delete'):
        add('live-'+scene,'Live '+scene+' updates visible IDs',scene,lambda s,_:sorted(s['nodes']),source='data',severity=2)
        add('retention-'+scene,'Survivor world-coordinate displacement at update delivery: '+scene,scene,
            lambda s,side:retention(s,side),tol=.05,unit='0.05 world unit at delivery',source='flow',severity=2)
    add('new-placement','New file distance from its surviving hub neighbour at delivery','save',lambda s,side:placement(s,side),tol=15,unit='15 world units; independent seeded placement',source='flow')
    add('buffer-200','Unsaved-buffer visible IDs after 200 ms','buffer',lambda s,_:sorted(s['extra']['at200']['nodes']),source='flow',severity=2)
    add('buffer-600','Unsaved-buffer visible IDs after 600 ms','buffer',lambda s,_:sorted(s['extra']['at600']['nodes']),source='data')
    missing('buffer-host-debounce','Application edit-to-graph 400 ms debounce','Direct reference set_text runs immediately; the editor-to-widget debounce lives in the full window. The measured widget-level 200/600 ms rows must not be treated as application-level timing.','flow')
    add('font-live','Changing font family changes the focused label pixels','font',lambda s,side:font_changed(out,side,s),source='draw')
    add('resize','Dock-sized widget resize preserves centre and mapped graph','resize',lambda s,_:[s['state']['size'],pan_delta(s),s['mapped']],tol=.1,unit='0.1 logical px',source='camera')
    add('hidden','No frames advance during one second while unmapped','hidden',lambda s,_:s['extra']['at1000'].get('frames',s['extra']['at1000'].get('ticks',0))-s['extra']['at0'].get('frames',s['extra']['at0'].get('ticks',0)),source='flow',severity=1)
    add('shown','Showing the hidden widget restores a nonempty mapped graph','shown',lambda s,_:[s['mapped'],s['state']['nodes'],s['panel_open']],source='blank',severity=1)
    add('idle-cpu','Two-second settled CPU ticks (100 ticks per second on this machine)','idle',lambda s,_:s['extra']['cpu_ticks'],tol=2,unit='2 ticks; page descendants included on reference side',source='flow')
    add('persistence','Persisted blob restores search, panel, zoom and all graph preferences','persist',lambda s,_:s['extra']['restored'],tol=1e-8,unit='1e-8 numeric; strings/flags exact',source='panel')
    missing('persistence-restart','Persistence through a full application process restart','The paired drivers reconstruct widgets from their captured stores; a full editor/settings-process restart is outside the widget harness.','flow')
    missing('persistence-delay','Exact preference write debounce and zoom-save time','Saved payloads are measured; real disk writes and host settings debounce are replaced by isolated stores to avoid touching user configuration.','flow')
    add('click-visible','Graph remains nonempty and mapped after pointer/keyboard actions','key-escape-again',lambda s,_:[s['mapped'],s['state']['nodes']>0],source='blank',severity=1)
    # Screenshot centre comparisons must compare only the shared unoccluded kind samples.
    for row in rows:
        if row['id'].startswith('pixels-') and row['verdict']!='unmeasurable':
            common=row['reference'].keys() & row['impro'].keys()
            if not common:
                row.update(verdict='unmeasurable',reason='No shared unoccluded marker centre lies inside both frames.')
            else:
                row['reference']={k:row['reference'][k] for k in sorted(common)}
                row['impro']={k:row['impro'][k] for k in sorted(common)}
                row['verdict']='same' if same(row['reference'],row['impro'],3) else 'different'
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
