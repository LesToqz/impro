#!/usr/bin/env python3
"""Run the read-only reference graph widget, discovering its package at runtime."""
import argparse
import importlib
import json
import os
from pathlib import Path
import sys
import time


def main():
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument('--reference', type=Path, required=True)
    parser.add_argument('--root', type=Path, required=True)
    parser.add_argument('--actions', type=Path, required=True)
    parser.add_argument('--out', type=Path, required=True)
    args = parser.parse_args()
    packages = [p for p in args.reference.iterdir() if (p/'graphview.py').is_file()]
    if len(packages) != 1:
        raise RuntimeError('expected exactly one graph widget package')
    sys.path.insert(0, str(args.reference))
    def module(name):
        return importlib.import_module(packages[0].name+'.'+name)
    import gi
    if os.getenv('IMPRO_PARITY_GI_PATH'):
        gi.__path__.append(os.environ['IMPRO_PARITY_GI_PATH'])
    gi.require_foreign('cairo')
    gi.require_version('Gtk', '3.0')
    gi.require_version('Gdk', '3.0')
    from gi.repository import Gtk, Gdk, GLib
    preferences = module('settings')
    preferences._path = lambda: Path(os.environ['XDG_CONFIG_HOME'])/'impro/settings.json'
    preferences.get_value = lambda key: {'graph_default_type':'global', 'reduce_motion':False}.get(key)
    preferences.get_graph = lambda: {}
    preferences.set_graph = lambda value: saved.append(dict(value))
    notes = module('notes')
    notes.NOTES_DIR = os.path.join('.impro', 'Notes')
    saved = []
    themes = module('themes')
    graphmod = module('graphview')
    native = Gtk.Settings.get_default()
    native.set_property('gtk-font-name', 'DejaVu Sans 10')
    native.set_property('gtk-cursor-blink', False)
    graph = graphmod.GraphView(str(args.root), settings={})
    window = Gtk.Window(title='impro graph audit')
    window.set_default_size(1000, 700)
    window.add(graph)
    errors = []
    graph.connect('error', lambda _, message: errors.append(message))
    events = []
    # Observe raw page messages as well as letting the real host handle them.
    def message(_, result):
        m = json.loads(result.get_js_value().to_json(0))
        if m.get('type') == 'open':
            events.append(dict(type='open', id=m['id'], mode='new-tab' if m.get('newTab') else 'current'))
        elif m.get('type') == 'search':
            events.append(dict(type='search', query=m['query']))
        elif m.get('type') == 'create':
            events.append(dict(type='create', name=m['name']))
    # The bridge name is obtained from its installed runtime registration.
    source = (packages[0]/'graphview.py').read_text()
    import re
    handler = re.search(r"register_script_message_handler\(['\"]([^'\"]+)", source).group(1)
    graph._manager.connect('script-message-received::'+handler, message)
    window.show_all()
    window.move(0, 0)
    context = GLib.MainContext.default()
    def pump(ms):
        end = time.monotonic()+ms/1000
        while True:
            while context.pending():
                context.iteration(False)
            if time.monotonic() >= end:
                return
            time.sleep(.002)
    def poll(predicate, seconds=15):
        end = time.monotonic()+seconds
        while not predicate():
            if time.monotonic() > end:
                raise RuntimeError('readiness deadline exceeded')
            pump(5)
    def js(script):
        done = []
        def finished(web, result, _):
            try:
                done.append((True, json.loads(web.evaluate_javascript_finish(result).to_json(0))))
            except Exception as error:
                done.append((False, str(error)))
        graph.webview.evaluate_javascript(script, -1, None, None, None, finished, None)
        poll(lambda: done)
        if not done[0][0]:
            raise RuntimeError(done[0][1])
        return done[0][1]
    poll(lambda: graph._ready and graph.last_data is not None)
    api = "Object.values(window).find(v=>v && typeof v.stats==='function' && v._debug)"
    js('window.auditAPI='+api+'; true')
    def invoke(command, value=None):
        return js('(()=>{auditAPI['+json.dumps(command)+']('+json.dumps(value)+');return true})()')
    def state():
        return js("(()=>{const s=auditAPI.stats();return {zoom:s.scale,target_zoom:s.targetScale,pan:s.pan,size:[s.width,s.height],nodes:s.nodes,lines:s.links,hover:s.hover,dragging:document.querySelector('#graph').classList.contains('is-grabbing'),simulating:s.simRunning,alpha:s.alpha,running:s.running,frames:s.frames}})()")
    def settle():
        pump(120)
        poll(lambda: not state()['running'], 15)
        pump(60)
    def prefs(values):
        graph.set_settings(dict(graph._settings, **values))
        pump(220)
        settle()
    def theme(name):
        th = themes.get_theme(themes.DEFAULT_THEME_ID.rsplit('-', 1)[0]+'-'+name)
        graph.set_theme(th.ui, th.ui['accent'])
        # Font role delivered live by the host theme bridge.
        invoke('setTheme', dict(graphmod.graph_colors(th.ui, th.ui['accent']),
                               accent=th.ui['accent'], font='DejaVu Sans'))
        settle()
    def snapshot():
        result = js("""(()=>{
          const a=auditAPI,d=a._debug, q=s=>document.querySelector(s);
          const sliders={};
          for(const e of document.querySelectorAll('input[type=range]')) {
            sliders[e.getAttribute('aria-label')]=[e.getAttribute('aria-label'),+e.min,+e.max,+e.step,+e.value];
          }
          const keys=['localDepth','textFadeMultiplier','nodeSizeMultiplier','lineSizeMultiplier','centerStrength','repelStrength','linkStrength','linkDistance'];
          const ranges={}; Object.values(sliders).forEach((v,i)=>ranges[keys[i]]=v);
          const visible=e=>e && !e.hidden && e.style.display!=='none';
          const rows=Array.from(document.querySelectorAll('#controls .row'));
          const row=label=>rows.find(e=>e.querySelector('.name')?.textContent.startsWith(label));
          return {nodes:Object.fromEntries(d.ids().map(id=>[id,{world:d.worldPos(id),screen:d.screenPos(id),alpha:d.nodeAlpha(id),color:d.nodeColor(id)}])),
            prefs:d.settings(),panel_open:d.panelOpen(),
            sections:Array.from(document.querySelectorAll('.sec')).map(e=>[e.querySelector('.sec-head>span:not(.chev)').textContent,e.classList.contains('is-collapsed')]),
            sliders:ranges,labels:Array.from(document.querySelectorAll('#controls .name')).map(e=>e.firstElementChild ? e.firstElementChild.textContent : e.textContent),
            status:visible(q('#status'))?q('#status').textContent:'', notice:visible(q('#notice'))?q('#notice').textContent:'',
            graph_type:d.graphType().type,local_rows:['Neighbourhood hops','Referenced by','References to','Unlinked files'].map(t=>visible(row(t))),
            background:getComputedStyle(document.body).backgroundColor,
            cursor:getComputedStyle(q('#graph')).cursor};
        })()""")
        result['state'] = state()
        result['mapped'] = graph.get_mapped()
        return result
    def point(node):
        p = js('auditAPI._debug.screenPos('+json.dumps(node)+')')
        if p is None:
            raise RuntimeError('missing marker '+node)
        return p[:2]
    def pointer(kind, x, y, button=1, ctrl=False):
        return js("(()=>{const c=document.querySelector('#graph'),r=c.getBoundingClientRect();c.dispatchEvent(new PointerEvent("+json.dumps(kind)+",{bubbles:true,pointerId:1,button:"+str(button-1)+",clientX:r.left+"+str(x)+",clientY:r.top+"+str(y)+",ctrlKey:"+str(ctrl).lower()+"}));return true})()")
    def key(k, down, shift=False):
        target = "document.querySelector('#graph')" if down else 'window'
        return js('(()=>{'+target+'.dispatchEvent(new KeyboardEvent('+json.dumps('keydown' if down else 'keyup')+',{key:'+json.dumps(k)+',shiftKey:'+str(shift).lower()+',bubbles:true}));return true})()')
    def click_button(selector):
        js('(()=>{document.querySelector('+json.dumps(selector)+').click();return true})()')
    def capture(name):
        pump(80)
        w, h = graph.get_allocated_width(), graph.get_allocated_height()
        pixbuf = Gdk.pixbuf_get_from_window(graph.webview.get_window(), 0, 0, w, h)
        if pixbuf is None:
            raise RuntimeError('capture failed')
        pixbuf.savev(str(args.out/(name+'.png')), 'png', [], [])
    theme('dark')
    start = time.monotonic()
    settle()
    initial_settle_ms = (time.monotonic()-start)*1000
    steps = {}
    args.out.mkdir(parents=True, exist_ok=True)
    for action in json.loads(args.actions.read_text()):
        op, name = action['op'], action.get('name', '')
        print('audit', op, name, file=sys.stderr, flush=True)
        before, event_start = snapshot(), len(events)
        started, extra = time.monotonic(), {}
        if op == 'capture':
            pass
        elif op == 'prefs':
            prefs({k:v for k,v in action.items() if k not in ('op','name')})
        elif op == 'theme':
            theme(action['theme'])
        elif op == 'active':
            graph.set_active_file(str(args.root/action['id']) if action['id'] else None)
            settle()
        elif op == 'hover':
            pointer('pointermove', *point(action['id']))
            settle()
        elif op == 'leave':
            pointer('pointerleave', -1000, -1000)
            settle()
        elif op == 'click':
            x,y = point(action['id'])
            button,ctrl = action.get('button',1),action.get('ctrl',False)
            pointer('pointerdown',x,y,button,ctrl)
            dx = action.get('move',0)
            if dx:
                pointer('pointermove',x+dx,y,button,ctrl)
            pointer('pointerup',x+dx,y,button,ctrl)
            if button == 3:
                js("(()=>{const c=document.querySelector('#graph'),r=c.getBoundingClientRect();c.dispatchEvent(new MouseEvent('contextmenu',{bubbles:true,clientX:r.left+"+str(x)+",clientY:r.top+"+str(y)+"}));return true})()")
            settle()
        elif op == 'menu':
            x,y = point(action['id'])
            js("(()=>{const c=document.querySelector('#graph'),r=c.getBoundingClientRect();c.dispatchEvent(new MouseEvent('contextmenu',{bubbles:true,clientX:r.left+"+str(x)+",clientY:r.top+"+str(y)+"}));return true})()")
            extra['menu'] = js("Array.from(document.querySelectorAll('#menu .item')).map(e=>e.textContent)")
            key('Escape',True)
        elif op == 'drag':
            x,y = point(action['id'])
            pointer('pointerdown',x,y)
            pointer('pointermove',x+60,y+40)
            pump(120)
            extra['held'] = snapshot()
            pointer('pointerup',x+60,y+40)
            settle()
        elif op == 'pan':
            pointer('pointerleave',-1000,-1000)
            pointer('pointerdown',2,2)
            for i in range(1,9):
                pointer('pointermove',2+10*i,2+4*i)
                pump(16)
            extra['held'] = state()
            pointer('pointerup',82,34)
            pump(120)
            extra['after120'] = state()
            settle()
        elif op == 'stop':
            pointer('pointerdown',2,2)
            pointer('pointermove',62,2)
            pointer('pointerup',62,2)
            pump(32)
            pointer('pointerdown',2,2)
            extra['pressed'] = state()
            pump(200)
            extra['later'] = state()
            pointer('pointerup',2,2)
            settle()
        elif op == 'wheel':
            js("(()=>{const c=document.querySelector('#graph'),r=c.getBoundingClientRect();c.dispatchEvent(new WheelEvent('wheel',{bubbles:true,cancelable:true,clientX:r.left+200,clientY:r.top+150,deltaY:"+str(action['delta'])+",ctrlKey:"+str(action['ctrl']).lower()+"}));return true})()")
            extra['immediate'] = state()
            settle()
        elif op == 'key':
            key(action['key'],True,action.get('shift',False))
            pump(200)
            key(action['key'],False)
            if action.get('shift'):
                key('Shift',False)
            settle()
        elif op == 'search':
            js("(()=>{const e=document.querySelector('.search input');e.value="+json.dumps(action['text'])+";e.dispatchEvent(new Event('input',{bubbles:true}));return true})()")
            pump(350)
            settle()
        elif op == 'local':
            graph.set_active_file(str(args.root/action['id']))
            graph.show_local(str(args.root/action['id']),action['depth'])
            pump(250)
            settle()
        elif op == 'global':
            graph.show_global()
            graph.set_active_file(None)
            pump(250)
            settle()
        elif op == 'local-empty':
            graph.set_active_file(None)
            graph.set_graph_type('local')
            pump(250)
            settle()
        elif op == 'note':
            graph.set_graph_type('note')
            pump(250)
            settle()
        elif op == 'group-add':
            click_button('.groups-new')
            pump(250)
            settle()
        elif op == 'group-remove':
            prefs(dict(colorGroups=[]))
        elif op == 'reset':
            click_button('[data-tip="Reset graph preferences"]')
            pump(250)
            settle()
        elif op == 'replay':
            click_button('[data-tip="Replay project history"]')
            pump(250)
            extra['at250'] = snapshot()
            pump(750)
            extra['at1000'] = snapshot()
            settle()
        elif op == 'buffer':
            graph.set_text(str(args.root/action['id']),action['text'])
            pump(200)
            extra['at200'] = snapshot()
            pump(400)
            extra['at600'] = snapshot()
            settle()
        elif op == 'buffer-clear':
            graph.set_text(str(args.root/action['id']),None)
            pump(700)
            settle()
        elif op == 'save':
            (args.root/'saved.md').write_text('[[hub/Hub]]\n')
            graph.refresh_file(str(args.root/'saved.md'))
            pump(250)
            settle()
        elif op == 'rename':
            (args.root/'saved.md').rename(args.root/'renamed.md')
            graph._scan_needed = True
            graph.rebuild()
            pump(250)
            settle()
        elif op == 'delete':
            (args.root/'renamed.md').unlink()
            graph._scan_needed = True
            graph.rebuild()
            pump(250)
            settle()
        elif op == 'resize':
            window.resize(800,600)
            pump(250)
            settle()
        elif op == 'hidden':
            window.hide()
            pump(100)
            extra['at0'] = state()
            pump(1000)
            extra['at1000'] = state()
            window.show_all()
            pump(200)
            settle()
        elif op == 'idle':
            # Parent and existing descendants: include the page process in CPU evidence.
            def processes(pid):
                found = [pid]
                try:
                    children = Path(f'/proc/{pid}/task/{pid}/children').read_text().split()
                except FileNotFoundError:
                    return found
                for child in children:
                    found.extend(processes(int(child)))
                return found
            def ticks():
                total=0
                for pid in processes(os.getpid()):
                    try:
                        cols=Path(f'/proc/{pid}/stat').read_text().rsplit(')',1)[1].split()
                        total+=int(cols[11])+int(cols[12])
                    except FileNotFoundError:
                        pass
                return total
            cpu=ticks()
            start=time.monotonic()
            pump(2000)
            extra.update(cpu_ticks=ticks()-cpu,wall_ms=(time.monotonic()-start)*1000)
        elif op == 'persist':
            prefs(dict(close=True, search='path:hub'))
            invoke('_debug') if False else None
            js('(()=>{auditAPI._debug.flush();return true})()')
            pump(800)
            extra['saved'] = saved[-1] if saved else None
            # Recreate the real widget from the captured persisted blob, without a new page run.
            restored=graphmod.GraphView(str(args.root),settings=extra['saved'])
            extra['restored']=restored._settings
            restored.shutdown()
            restored.destroy()
        else:
            raise ValueError('unknown operation '+op)
        if name:
            s=snapshot()
            s.update(before=before,extra=extra,events=events[event_start:],elapsed_ms=(time.monotonic()-started)*1000)
            if action.get('png'):
                capture(name)
            steps[name]=s
            (args.out/'measurements.json').write_text(json.dumps(dict(steps=steps,errors=errors,initial_settle_ms=initial_settle_ms),indent=2))
    graph.shutdown()
    window.destroy()


if __name__ == '__main__':
    main()
