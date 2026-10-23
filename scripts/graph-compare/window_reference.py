#!/usr/bin/env python3
"""Full-window buffer and real-settings oracle, with isolated configuration."""
import argparse
import importlib
import json
import math
import os
from pathlib import Path
import sys
import time


def main():
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument('--reference', type=Path, required=True)
    parser.add_argument('--root', type=Path, required=True)
    parser.add_argument('--out', type=Path, required=True)
    parser.add_argument('--stage', choices=('save','restore'), required=True)
    args = parser.parse_args()
    args.out.mkdir(parents=True,exist_ok=True)
    packages = [p for p in args.reference.iterdir() if (p/'window.py').is_file() and (p/'graphview.py').is_file()]
    if len(packages)!=1:
        raise RuntimeError('expected one reference application package')
    sys.path.insert(0,str(args.reference))
    module = lambda name: importlib.import_module(packages[0].name+'.'+name)
    import gi
    if os.getenv('IMPRO_PARITY_GI_PATH'):
        gi.__path__.append(os.environ['IMPRO_PARITY_GI_PATH'])
    gi.require_foreign('cairo')
    gi.require_version('Gtk','3.0')
    from gi.repository import Gtk,Gio,GLib
    settings = module('settings')
    config = Path(os.environ['XDG_CONFIG_HOME'])/'impro/settings.json'
    settings._path = lambda: config
    module('notes').NOTES_DIR = os.path.join('.impro','Notes')
    args.root.mkdir(parents=True,exist_ok=True)
    if args.stage=='save':
        (args.root/'a.md').write_text('[[b]] #topic\n')
        (args.root/'b.md').write_text('[[a]]\n')
        (args.root/'other.md').write_text('unrelated\n')
        if not settings.save({'reduce_motion':True,'graph':{'scale':.04,'close':True}}):
            raise RuntimeError('could not seed isolated settings')
    native = Gtk.Settings.get_default()
    native.set_property('gtk-enable-animations',False)
    native.set_property('gtk-cursor-blink',False)
    app = Gtk.Application(application_id='dev.impro.GraphWindowOracle',flags=Gio.ApplicationFlags.NON_UNIQUE)
    app.register(None)
    window = module('window').MainWindow(app,str(args.root))
    window.set_title('impro')
    window.show_all()
    context = GLib.MainContext.default()
    def pump(ms):
        end = time.monotonic()+ms/1000
        while True:
            while context.pending():
                context.iteration(False)
            if time.monotonic()>=end:
                return
            time.sleep(.002)
    def poll(predicate,seconds=20):
        end=time.monotonic()+seconds
        while not predicate():
            if time.monotonic()>end:
                raise RuntimeError('window oracle deadline exceeded')
            pump(2)
    def frame():
        seen=[]
        signal=window.connect_after('draw',lambda *_: seen.append(True) or False)
        window.queue_draw()
        poll(lambda:seen)
        window.disconnect(signal)
    graph = window.open_graph()
    poll(lambda:graph._ready and graph.last_data is not None)
    def js(script):
        done=[]
        def finished(web,result,_):
            try:
                done.append((True,json.loads(web.evaluate_javascript_finish(result).to_json(0))))
            except Exception as error:
                done.append((False,str(error)))
        graph.webview.evaluate_javascript(script,-1,None,None,None,finished,None)
        poll(lambda:done)
        if not done[0][0]:
            raise RuntimeError(done[0][1])
        return done[0][1]
    js("window.auditAPI=Object.values(window).find(v=>v && typeof v.stats==='function' && v._debug); true")
    poll(lambda:not js('auditAPI.stats().running'))
    def record(name,value):
        (args.out/(name+'.json')).write_text(json.dumps(value,indent=2))
    def disk():
        return json.loads(config.read_text())['graph']
    expected=.04 if args.stage=='save' else .04*math.exp(.25)
    assert abs(js('auditAPI.stats().scale')-expected)<1e-8
    assert not js('auditAPI._debug.panelOpen()')
    if args.stage=='restore':
        result=js("""(()=>{const s=auditAPI.stats(),d=auditAPI._debug;
          return {scale:s.scale,close:!d.panelOpen(),showTags:d.settings().showTags,
            chip_visible:getComputedStyle(document.querySelector('.gc-chip')).display!=='none',
            visible:d.ids().some(id=>{const p=d.screenPos(id);return p[0]>=0&&p[0]<=s.width&&p[1]>=0&&p[1]<=s.height}),fresh_process:true};})()""")
        assert all(result[k] for k in ('close','showTags','chip_visible','visible'))
        record('settings-restore',result)
    else:
        frame()
        assert not disk()['showTags']
        started=time.monotonic()
        js("document.querySelector('[role=switch][aria-label=\"Tag markers\"]').click(); true")
        pump(40)
        assert not disk()['showTags'], f'preference sample at {(time.monotonic()-started)*1000:.1f} ms'
        poll(lambda:disk()['showTags'])
        preference_delay_ms=(time.monotonic()-started)*1000
        started=time.monotonic()
        js("document.querySelector('[role=switch][aria-label=\"Reference direction\"]').click(); true")
        pump(120)
        assert disk()['showArrow']
        poll(lambda:not disk()['showArrow'])
        display_delay_ms=(time.monotonic()-started)*1000
        started=time.monotonic()
        js("document.querySelector('#graph').dispatchEvent(new WheelEvent('wheel',{bubbles:true,cancelable:true,clientX:180,clientY:120,deltaY:-120})); true")
        pump(600)
        assert disk()['scale']==.04
        poll(lambda:abs(disk()['scale']-.04*math.exp(.25))<1e-8)
        zoom_delay_ms=(time.monotonic()-started)*1000
        record('settings-save',dict(preference_delay_ms=preference_delay_ms,display_delay_ms=display_delay_ms,
                                    zoom_delay_ms=zoom_delay_ms,pref_absent_at40=True,
                                    display_absent_at120=True,zoom_absent_at600=True))
        window.editor.open_file(str(args.root/'a.md'))
        poll(lambda:window.editor.current_path()==str(args.root/'a.md'))
        a=next(tab for tab in window.editor.tabs if tab.path==str(args.root/'a.md'))
        frame()
        started=time.monotonic()
        a.buffer.set_text('[[Draft Target]]\n')
        pump(200)
        assert '?Draft Target' not in js('auditAPI._debug.ids()')
        poll(lambda:'?Draft Target' in js('auditAPI._debug.ids()'))
        elapsed_ms=(time.monotonic()-started)*1000
        window.open_local_graph()
        poll(lambda:'?Draft Target' in js('auditAPI._debug.ids()'))
        window.editor.open_file(str(args.root/'b.md'))
        poll(lambda:window.editor.current_path()==str(args.root/'b.md'))
        frame()
        poll(lambda:'b.md' in js('auditAPI._debug.ids()') and '?Draft Target' not in js('auditAPI._debug.ids()'))
        window.editor.open_file(str(args.root/'a.md'))
        poll(lambda:window.editor.current_path()==str(args.root/'a.md'))
        frame()
        poll(lambda:'?Draft Target' in js('auditAPI._debug.ids()'))
        record('buffer',dict(at200=False,after=True,elapsed_ms=elapsed_ms,local_follow=True))
    graph.shutdown()
    window.destroy()


if __name__=='__main__':
    main()
