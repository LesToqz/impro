#[cfg(unix)]
#[path = "../tests/common/mod.rs"]
mod common;
#[cfg(unix)]
use common::*;
#[cfg(unix)]
use gtk::prelude::*;
// This standalone QA host also supports WebKit feature levels before 2.40.
#[cfg(unix)]
#[allow(deprecated)]
fn main() {
    gtk::init().expect("GTK display");
    {
        use webkit2gtk::{LoadEvent, WebView, WebViewExt};
        fn js(web: &WebView, script: &str) {
            let result = std::rc::Rc::new(std::cell::RefCell::new(None));
            let out = result.clone();
            web.run_javascript(script, None::<&gio::Cancellable>, move |value| {
                *out.borrow_mut() = Some(value.map(|_| ()))
            });
            poll(|| result.borrow().is_some());
            result.borrow_mut().take().unwrap().unwrap();
        }
        for stacked in [false, true] {
            let d = Dock::new(stacked, 1);
            let web = WebView::new();
            let ready = std::rc::Rc::new(std::cell::Cell::new(false));
            let loads = std::rc::Rc::new(std::cell::Cell::new(0));
            let r = ready.clone();
            let l = loads.clone();
            web.connect_load_changed(move |_, e| {
                l.set(l.get() + 1);
                if e == LoadEvent::Finished {
                    r.set(true);
                }
            });
            web.load_html("<html><body><script>window.paneState={camera:123,selection:'kept'};window.calls=[];</script>impro</body></html>",None);
            let r = ready.clone();
            web.connect_map(move |w| {
                if r.get() {
                    w.run_javascript("calls.push('resume')", None::<&gio::Cancellable>, |_| {});
                }
            });
            let r = ready.clone();
            web.connect_unmap(move |w| {
                if r.get() {
                    w.run_javascript("calls.push('pause')", None::<&gio::Cancellable>, |_| {});
                }
            });
            assert!(d.panel.add_gitgraph(Some(web.upcast_ref())));
            poll(|| ready.get());
            let count = loads.get();
            let body = d.panel.body();
            for _ in 0..8 {
                body.move_handle(0, -8.);
                pump(20);
            }
            js(
                &web,
                "if(paneState.camera!==123||paneState.selection!=='kept')throw Error('lost state')",
            );
            d.panel.toggle_maximize_pane(d.panel.panes()[0].id);
            pump(80);
            assert!(!web.is_mapped());
            d.panel.restore();
            pump(80);
            assert!(web.is_mapped());
            js(&web,"if(paneState.camera!==123||!calls.includes('pause')||!calls.includes('resume'))throw Error('map state')");
            assert_eq!(loads.get(), count);
            let a = body.allocation();
            body.size_allocate(&gtk::Allocation::new(a.x(), a.y(), 1, 1));
            body.queue_resize();
            pump(80);
            js(
                &web,
                "if(paneState.camera!==123)throw Error('collapse state')",
            );
            assert_eq!(loads.get(), count);
        }
    }
    pump(300);
}

#[cfg(not(unix))]
fn main() {}
