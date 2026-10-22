use super::common::*;
use gtk::prelude::*;
use impro_core::themes::{contrast, get_theme};
use impro_gitviews::palette::git_colors;
use serde_json::{json, Value};
fn key(p: &Page, key: &str) {
    p.js(&format!("(()=>{{const c=document.getElementById('gg');c.focus();c.dispatchEvent(new KeyboardEvent('keydown',{{key:{},bubbles:true,cancelable:true}}));}})()",json!(key)));
}
fn pointer(p: &Page, kind: &str, hash: &Value, buttons: i32) {
    p.js(&format!("(()=>{{const [x,y]=ImproGit._debug.screenPos({hash});const c=document.getElementById('gg');const r=c.getBoundingClientRect();c.dispatchEvent(new PointerEvent({},{{bubbles:true,cancelable:true,pointerId:1,pointerType:'mouse',isPrimary:true,clientX:r.left+x,clientY:r.top+y,button:0,buttons:{buttons}}}));}})()",json!(kind)));
}
fn cpu_ticks() -> u64 {
    std::fs::read_dir("/proc")
        .unwrap()
        .flatten()
        .filter_map(|entry| {
            let stat = std::fs::read_to_string(entry.path().join("stat")).ok()?;
            let end = stat.rfind(')')?;
            if !stat[..end].contains("WebKitWeb") {
                return None;
            }
            let f: Vec<_> = stat[end + 2..].split_whitespace().collect();
            if f.get(1)?.parse::<u32>().ok() != Some(std::process::id()) {
                return None;
            }
            Some(f[11].parse::<u64>().ok()? + f[12].parse::<u64>().ok()?)
        })
        .sum()
}
pub fn tests() {
    let p = Page::new(false, true);
    let data = fixture("gitgraph_showcase.json");
    let head = &data["head"]["hash"];
    run("test_01_api_ready_and_safe_before_data", || {
        for cmd in [
            "setTheme",
            "setSettings",
            "select",
            "focus",
            "pause",
            "resume",
            "setData",
            "setState",
            "stats",
        ] {
            p.check(&format!("typeof ImproGit.{cmd}==='function'"));
        }
        assert_eq!(p.count("ready"), 1);
        p.check("ImproGit.stats().state==='loading'");
        p.check("document.getElementById('state').innerText.includes('Loading history')");
    });
    run("test_02_renders_showcase", || {
        p.graph();
        assert_eq!(
            p.js("ImproGit.stats().commits"),
            data["commits"].as_array().unwrap().len()
        );
        p.check("ImproGit.stats().lanes>=6&&!ImproGit.stats().running");
        p.check("ImproGit._debug.labels().some(l=>l.kind==='head')&&ImproGit._debug.labels().some(l=>l.kind==='pill')");
        p.shot("graph-horizontal-dark");
    });
    run("test_03_hover_tooltip_hash_subject_time", || {
        let hash = data["commits"]
            .as_array()
            .unwrap()
            .iter()
            .find(|c| c["subject"] == "Merge branch 'feature/ui'")
            .unwrap()["hash"]
            .clone();
        pointer(&p, "pointermove", &hash, 0);
        p.wait("ImproGit.stats().tip");
        p.check("ImproGit._debug.tipParts()[0].cls.startsWith('t-hash')&&ImproGit._debug.tipParts()[1].cls==='t-subj'&&ImproGit._debug.tipParts()[2].cls==='t-time'");
        p.check("(()=>{const r=ImproGit._debug.tipRect(),s=ImproGit.stats();return r.x>=0&&r.y>=0&&r.x+r.w<=s.width&&r.y+r.h<=s.height})()");
        p.js("document.getElementById('gg').dispatchEvent(new PointerEvent('pointerleave',{clientX:-5,clientY:-5}));");
        pump(200);
        p.check("!ImproGit.stats().tip");
    });
    run("test_04_click_posts_open_commit_and_select_ring", || {
        let hash = data["commits"][3]["hash"].clone();
        pointer(&p, "pointerdown", &hash, 1);
        pointer(&p, "pointerup", &hash, 0);
        pump(50);
        assert_eq!(p.count("open-commit"), 1);
        assert_eq!(p.js("ImproGit.stats().selected"), hash);
        p.call("select", &[Value::Null]);
        assert_eq!(p.js("ImproGit.stats().selected"), Value::Null);
        p.call("select", std::slice::from_ref(&hash));
        assert_eq!(p.js("ImproGit.stats().selected"), hash);
    });
    run("test_05_head_pill_over_head_commit", || {
        p.check(&format!("(()=>{{const [x,y]=ImproGit._debug.screenPos({head}),h=ImproGit._debug.labels().filter(l=>l.kind==='head');return h.length===1&&Math.abs(h[0].x+h[0].w/2-x)<1.5&&h[0].y+h[0].h<y-9&&h[0].y+h[0].h>y-40;}})()"));
    });
    run("test_06_theme_dark_and_light", || {
        for (id, expected) in [
            ("impro-light", "rgb(244, 247, 245)"),
            ("impro-dark", "rgb(24, 33, 38)"),
        ] {
            let theme = get_theme(Some(id));
            p.call("setTheme", &[git_colors(&theme.ui, &theme.ui["accent"])]);
            pump(50);
            assert_eq!(
                p.js("getComputedStyle(document.body).backgroundColor"),
                expected
            );
            let palette = p.js("ImproGit._debug.palette()");
            for lane in palette["lanes"]
                .as_array()
                .unwrap()
                .iter()
                .chain(std::iter::once(&palette["main"]))
            {
                assert!(
                    contrast(
                        lane["text"].as_str().unwrap(),
                        lane["fill"].as_str().unwrap()
                    ) >= 4.5
                );
                assert!(
                    contrast(
                        lane["css"].as_str().unwrap(),
                        palette["bg"].as_str().unwrap()
                    ) >= 2.9
                );
            }
            assert!(
                contrast(
                    palette["headFg"].as_str().unwrap(),
                    palette["head"].as_str().unwrap()
                ) >= 4.5
            );
        }
    });
    run("test_07_states", || {
        let mut empty = data.clone();
        empty["commits"] = json!([]);
        p.call("setData", &[empty]);
        for (kind, text) in [
            ("loading", "Loading history"),
            ("empty", "No commits yet"),
            ("not-a-repo", "Not a Git repository"),
            ("git-missing", "Git is not installed"),
            ("error", "Could not read"),
        ] {
            p.call("setState", &[json!(kind), json!("boom")]);
            assert_eq!(p.js("ImproGit.stats().state"), kind);
            p.check(&format!("document.getElementById('state').innerText.includes({})&&!document.getElementById('state').hidden",json!(text)));
        }
        p.graph();
        p.call("setState", &[json!("loading")]);
        p.check("ImproGit.stats().state==='data'");
    });
    run("test_08_pause_resume_and_idle", || {
        p.graph();
        let frames = p.js("ImproGit.stats().frames");
        pump(350);
        assert_eq!(p.js("ImproGit.stats().frames"), frames);
        p.call("pause", &[]);
        p.call(
            "focus",
            &[data["commits"].as_array().unwrap().last().unwrap()["hash"].clone()],
        );
        pump(100);
        assert_eq!(p.js("ImproGit.stats().frames"), frames);
        p.check("ImproGit.stats().paused&&!ImproGit.stats().pulse");
        p.js("window.__timers=0;window.__timeout=window.setTimeout;window.setTimeout=(...a)=>{window.__timers++;return window.__timeout(...a)};true");
        p.call("setSettings", &[json!({"reduceMotion":false})]);
        p.call("setData", std::slice::from_ref(&data));
        p.call("focus", &[json!("HEAD")]);
        p.check("window.__timers===0");
        p.js("window.setTimeout=window.__timeout;true");
        p.call("resume", &[]);
        p.settle();
    });
    run("test_09_idle_cpu", || {
        p.graph();
        p.call("pause", &[]);
        pump(300);
        let before = cpu_ticks();
        let frames = p.js("ImproGit.stats().frames");
        pump(2000);
        let output = std::process::Command::new("getconf")
            .arg("CLK_TCK")
            .output()
            .unwrap();
        let hz = String::from_utf8_lossy(&output.stdout)
            .trim()
            .parse::<f64>()
            .unwrap();
        let paused = (cpu_ticks() - before) as f64 / hz / 2. * 100.;
        assert!(paused < 8.);
        assert_eq!(p.js("ImproGit.stats().frames"), frames);
        p.call("resume", &[]);
        p.settle();
        let before = cpu_ticks();
        pump(2000);
        let idle = (cpu_ticks() - before) as f64 / hz / 2. * 100.;
        assert!(idle < 8.);
        println!("paused-cpu-pct={paused:.2}; idle-cpu-pct={idle:.2}");
    });
    run("test_10_reduce_motion_is_instant", || {
        p.graph();
        p.check("!ImproGit.stats().animating");
        assert_eq!(p.js(&format!("ImproGit._debug.nodeAlpha({head})")), 1);
        p.call(
            "focus",
            &[data["commits"].as_array().unwrap().last().unwrap()["hash"].clone()],
        );
        p.check("!ImproGit.stats().animating");
        p.js("ImproGit._debug.pulseNow()");
        p.check("!ImproGit.stats().pulse");
    });
    run("test_11_new_commit_animates_in_and_head_glides", || {
        p.graph();
        p.call("setSettings", &[json!({"reduceMotion":false})]);
        let mut d = data.clone();
        let mut new = d["commits"][0].clone();
        new["hash"] = json!("f".repeat(40));
        new["short"] = json!("fffffff");
        new["parents"] = json!([head]);
        new["subject"] = json!("A brand new commit");
        new["refs"] = json!([{"name":"master","kind":"branch","current":true}]);
        for c in d["commits"].as_array_mut().unwrap() {
            c["refs"]
                .as_array_mut()
                .unwrap()
                .retain(|r| r["name"] != "master");
        }
        d["commits"].as_array_mut().unwrap().insert(0, new.clone());
        d["head"]["hash"] = new["hash"].clone();
        p.call("setData", &[d]);
        p.check("ImproGit.stats().animating");
        p.check(&format!("ImproGit._debug.nodeAlpha({})<0.95", new["hash"]));
        p.settle();
        assert_eq!(
            p.js(&format!("ImproGit._debug.nodeAlpha({})", new["hash"])),
            1
        );
    });
    run("test_12_keyboard_navigation", || {
        p.graph();
        key(&p, "End");
        assert_eq!(p.js("ImproGit.stats().cursor"), *head);
        key(&p, "ArrowLeft");
        p.check("ImproGit.stats().cursor!==ImproGit._debug.head()");
        key(&p, "Home");
        let n = p.count("open-commit");
        key(&p, "Enter");
        assert_eq!(p.count("open-commit"), n + 1);
        p.check("ImproGit.stats().tip");
        key(&p, "Escape");
        p.check("!ImproGit.stats().tip");
    });
    run("test_13_camera_wheel_fit_toolbar", || {
        p.graph();
        let scale = p.js("ImproGit.stats().scale").as_f64().unwrap();
        p.js("document.getElementById('gg').dispatchEvent(new WheelEvent('wheel',{bubbles:true,cancelable:true,clientX:500,clientY:260,deltaY:-360,deltaMode:0}));");
        p.settle();
        assert!(p.js("ImproGit.stats().scale").as_f64().unwrap() > scale * 1.3);
        p.js("document.getElementById('b-fit').click()");
        p.settle();
        assert!(p.js("ImproGit.stats().scale").as_f64().unwrap() < scale * 1.3);
        let n = p.count("refresh");
        p.js("document.getElementById('b-refresh').click()");
        assert_eq!(p.count("refresh"), n + 1);
        let n = p.js("ImproGit._debug.labels().length").as_u64().unwrap();
        p.js("document.getElementById('c-tags').click()");
        assert!(p.js("ImproGit._debug.labels().length").as_u64().unwrap() < n);
        p.js("document.getElementById('c-tags').click()");
    });
    run("test_14_small_and_large_panes", || {
        for (w, h) in [(420, 220), (2500, 900), (420, 700), (1100, 300)] {
            p.window.resize(w, h);
            p.web.set_size_request(w, h);
            pump(150);
            p.graph();
            p.check("ImproGit.stats().laneGap>=34");
            p.check(&format!("(()=>{{const [x,y]=ImproGit._debug.screenPos({head});return x>0&&y>0&&x<{w}&&y<{h};}})()"));
            p.check("(()=>{const a=ImproGit._debug.labels();return a.every((x,i)=>a.slice(i+1).every(y=>!(x.x<y.x+y.w&&y.x<x.x+x.w&&x.y<y.y+y.h&&y.y<x.y+x.h)))})()");
        }
        p.web.set_size_request(1100, 520);
        p.window.resize(1100, 520);
        pump(150);
    });
    run("test_15_fps_pan_300_and_1500", || {
        for n in [300, 1500] {
            p.call("setSettings", &[json!({"reduceMotion":true})]);
            p.call("setOrientation", &[json!("horizontal")]);
            p.js(&format!("ImproGit.setData(GitGraphCore.synth({n},3));ImproGit._debug.setView(ImproGit._debug.view().x,ImproGit._debug.view().y,0.6);true"));
            p.settle();
            assert_eq!(p.js("ImproGit.stats().commits"), n);
            p.js("window.__pan=null;(()=>{let frames=[],i=0;function f(t){frames.push(t);const v=ImproGit._debug.view();ImproGit._debug.setView(v.x+3,v.y,v.s);if(++i<90)requestAnimationFrame(f);else{const d=frames.slice(1).map((t,i)=>t-frames[i]).sort((a,b)=>a-b);window.__pan={fps:1000/((frames.at(-1)-frames[0])/(frames.length-1)),p95:d[Math.floor(d.length*.95)]}}}requestAnimationFrame(f)})();true");
            p.wait("window.__pan!==null");
            let result = p.js("window.__pan");
            println!("pan-{n}={result}");
            assert!(result["fps"].as_f64().unwrap() > if n == 300 { 30. } else { 15. });
        }
        p.graph();
    });
    run(
        "test_17_orientation_controls_animation_refresh_and_vertical_input",
        || {
            p.graph();
            p.call("select", std::slice::from_ref(head));
            p.call("setSettings", &[json!({"reduceMotion":false})]);
            let n = p.count("orientation");
            p.js("document.getElementById('b-vertical').click()");
            p.check("ImproGit.stats().animating");
            assert_eq!(p.count("orientation"), n + 1);
            p.settle();
            p.check("ImproGit.stats().orientation==='vertical'");
            assert_eq!(p.js("ImproGit.stats().selected"), *head);
            p.call("setOrientation", &[json!("horizontal")]);
            p.call("setOrientation", &[json!("vertical")]);
            p.settle();
            assert_eq!(p.count("orientation"), n + 1);
            key(&p, "End");
            key(&p, "ArrowDown");
            p.check("ImproGit.stats().cursor!==ImproGit._debug.head()");
            p.call(
                "setOptions",
                &[json!({"timeFormat":"absolute","showRemotes":false,"showTags":false})],
            );
            p.check("!ImproGit.stats().options.showTags&&!ImproGit.stats().options.showRemotes");
            p.shot("graph-vertical-dark");
            let theme = get_theme(Some("impro-light"));
            p.call("setTheme", &[git_colors(&theme.ui, &theme.ui["accent"])]);
            p.shot("graph-vertical-light");
            p.call("setOrientation", &[json!("horizontal")]);
            p.settle();
            p.shot("graph-horizontal-light");
        },
    );
    run(
        "test_18_orientation_reduced_motion_and_narrow_toolbar",
        || {
            p.call("setSettings", &[json!({"reduceMotion":true})]);
            for value in ["vertical", "horizontal", "vertical"] {
                p.call("setOrientation", &[json!(value)]);
                p.check("!ImproGit.stats().animating");
                assert_eq!(p.js("ImproGit.stats().orientation"), value);
            }
            p.web.set_size_request(240, 240);
            p.window.resize(240, 240);
            pump(150);
            for value in ["horizontal", "vertical"] {
                p.check(&format!("(()=>{{const r=document.getElementById('b-{value}').getBoundingClientRect();return r.left>=0&&r.right<=innerWidth&&r.top>=0&&r.bottom<=innerHeight;}})()"));
            }
        },
    );
    run("test_16_runs_without_bridge", || {
        let q = Page::new(false, false);
        q.graph();
        assert_eq!(q.js("ImproGit.outbox[0].type"), "ready");
        q.call("setData", &[fixture("gitgraph_deleted_branches.json")]);
        q.settle();
        q.check("ImproGit._debug.labels().some(l=>l.kind==='pill'&&l.text.startsWith('ceo/'))");
    });
    assert_eq!(p.count("error"), 0, "{:?}", p.messages.borrow());
}
