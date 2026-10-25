#![cfg(unix)]
#![allow(dead_code)]
use gtk::prelude::*;
use javascriptcore::ValueExt;
use serde_json::{json, Value};
use std::{
    cell::{Cell, RefCell},
    path::Path,
    rc::Rc,
    time::{Duration, Instant},
};
use webkit2gtk::{
    HardwareAccelerationPolicy, SettingsExt, UserContentManager, UserContentManagerExt, WebView,
    WebViewExt,
};

pub fn pump(ms: u64) {
    let end = Instant::now() + Duration::from_millis(ms);
    loop {
        while gtk::events_pending() {
            gtk::main_iteration_do(false);
        }
        if Instant::now() >= end {
            break;
        }
        std::thread::sleep(Duration::from_millis(2));
    }
}
pub fn poll(mut f: impl FnMut() -> bool) {
    let end = Instant::now() + Duration::from_secs(15);
    while !f() {
        assert!(Instant::now() < end, "GTK operation timed out");
        pump(5);
    }
}
pub fn js(web: &WebView, script: &str) -> Value {
    let output = Rc::new(RefCell::new(None));
    let out = output.clone();
    web.evaluate_javascript(script, None, None, gio::Cancellable::NONE, move |r| {
        *out.borrow_mut() = Some(r.map(|v| {
            v.to_json(0)
                .and_then(|v| serde_json::from_str(&v).ok())
                .unwrap_or(Value::Null)
        }));
    });
    poll(|| output.borrow().is_some());
    let value = output
        .borrow_mut()
        .take()
        .unwrap()
        .unwrap_or_else(|e| panic!("{e}: {script}"));
    value
}
pub fn fixture(name: &str) -> Value {
    let path = Path::new(env!("CARGO_MANIFEST_DIR"))
        .join("tests/fixtures")
        .join(name);
    serde_json::from_slice(&std::fs::read(path).unwrap()).unwrap()
}
pub struct Page {
    pub web: WebView,
    pub window: gtk::Window,
    pub messages: Rc<RefCell<Vec<Value>>>,
    pub api: &'static str,
}
impl Page {
    pub fn new(commit: bool, bridge: bool) -> Self {
        let manager = UserContentManager::new();
        let messages = Rc::new(RefCell::new(Vec::new()));
        if bridge {
            let m = messages.clone();
            manager.connect_script_message_received(Some("impro"), move |_, r| {
                if let Some(v) = r
                    .js_value()
                    .and_then(|v| v.to_json(0))
                    .and_then(|s| serde_json::from_str(&s).ok())
                {
                    m.borrow_mut().push(v);
                }
            });
            manager.register_script_message_handler("impro");
        }
        let web = WebView::builder().user_content_manager(&manager).build();
        WebViewExt::settings(&web)
            .unwrap()
            .set_hardware_acceleration_policy(HardwareAccelerationPolicy::Never);
        let window = gtk::Window::new(gtk::WindowType::Toplevel);
        window.set_default_size(
            if commit { 900 } else { 1100 },
            if commit { 1000 } else { 520 },
        );
        window.add(&web);
        window.show_all();
        let api = if commit { "ImproCommit" } else { "ImproGit" };
        web.load_html(
            &if commit {
                impro_gitviews::assets::commit_html()
            } else {
                impro_gitviews::assets::graph_html()
            },
            Some("about:blank"),
        );
        let loaded = Rc::new(Cell::new(false));
        let l = loaded.clone();
        web.connect_load_changed(move |_, event| {
            if event == webkit2gtk::LoadEvent::Finished {
                l.set(true);
            }
        });
        poll(|| loaded.get());
        let p = Self {
            web,
            window,
            messages,
            api,
        };
        p.wait(&format!("!!window.{api}"));
        p
    }
    pub fn js(&self, script: &str) -> Value {
        js(&self.web, script)
    }
    pub fn check(&self, expr: &str) {
        assert_eq!(self.js(&format!("!!({expr})")), true, "{expr}");
    }
    pub fn wait(&self, expr: &str) {
        poll(|| self.js(&format!("!!({expr})")) == true);
    }
    pub fn call(&self, name: &str, args: &[Value]) -> Value {
        self.js(&format!(
            "{}.{}({})",
            self.api,
            name,
            args.iter()
                .map(Value::to_string)
                .collect::<Vec<_>>()
                .join(",")
        ))
    }
    pub fn count(&self, kind: &str) -> usize {
        self.messages
            .borrow()
            .iter()
            .filter(|v| v["type"] == kind)
            .count()
    }
    pub fn settle(&self) {
        if self.api == "ImproGit" {
            self.wait("!ImproGit.stats().animating&&!ImproGit.stats().running");
        }
        pump(100);
    }
    pub fn graph(&self) {
        self.call("setSettings", &[json!({"reduceMotion":true})]);
        self.call("setData", &[fixture("gitgraph_showcase.json")]);
        self.js("ImproGit._debug.home()");
        self.settle();
    }
    pub fn commit(&self, name: &str) {
        self.call("setCommit", &[fixture(&format!("gitcommit_{name}.json"))]);
        pump(100);
    }
    pub fn shot(&self, name: &str) {
        if let Some(dir) = std::env::var_os("IMPRO_GIT_SHOTS") {
            pump(100);
            self.window.display().sync();
            let p = Path::new(&dir).join(format!("{name}.png"));
            std::fs::create_dir_all(&dir).unwrap();
            self.window
                .window()
                .unwrap()
                .pixbuf(
                    0,
                    0,
                    self.window.allocated_width(),
                    self.window.allocated_height(),
                )
                .unwrap()
                .savev(p, "png", &[])
                .unwrap();
        }
    }
}
impl Drop for Page {
    fn drop(&mut self) {
        self.web.terminate_web_process();
        unsafe {
            self.window.destroy();
        }
        pump(30);
    }
}

pub fn run(name: &str, test: impl FnOnce()) {
    print!("{name} ... ");
    test();
    println!("ok");
}
