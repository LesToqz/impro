use super::common::*;
use gtk::prelude::*;
use impro_core::themes::get_theme;
use impro_gitviews::palette::commit_colors;
use serde_json::{json, Value};
fn answer(p: &Page, req: &Value, empty: bool) {
    let start = req["start"].as_i64().unwrap();
    let end = req["end"].as_i64().unwrap();
    let lines: Vec<_> = if empty {
        vec![]
    } else {
        (start..=end)
            .map(|n| json!({"n":n,"text":format!("value_{n:03} = {n}"),"k":[[0,5,"variable"]]}))
            .collect()
    };
    p.call(
        "provideContext",
        &[req["path"].clone(), json!(start), json!(end), json!(lines)],
    );
}
fn request(p: &Page) -> Value {
    p.messages
        .borrow()
        .iter()
        .rev()
        .find(|v| v["type"] == "expand")
        .unwrap()
        .clone()
}
fn generated(n: usize) -> Value {
    let mut data = fixture("gitcommit_reference.json");
    let mut file = data["files"][1].clone();
    file["hunks"][0]["lines"]
        .as_array_mut()
        .unwrap()
        .truncate(100);
    file["hunks"][0]["new_lines"] = json!(100);
    data["files"] = json!((0..n)
        .map(|i| {
            let mut f = file.clone();
            f["path"] = json!(format!("src/module_{i:03}.py"));
            f
        })
        .collect::<Vec<_>>());
    data["stats"] = json!({"files":n,"additions":n*100,"deletions":0});
    data
}
pub fn tests() {
    let p = Page::new(true, true);
    let data = fixture("gitcommit_reference.json");
    let theme = get_theme(None);
    p.call(
        "setTheme",
        &[commit_colors(theme, &theme.ui, &theme.ui["accent"], 13)],
    );
    run("test_fixtures_are_real_contract_data", || {
        for name in ["reference", "gaps", "edges", "merge", "root", "empty"] {
            let d = fixture(&format!("gitcommit_{name}.json"));
            assert_eq!(d["ok"], true);
            assert_eq!(d["hash"].as_str().unwrap().len(), 40);
            assert_eq!(
                d["files"].as_array().unwrap().len(),
                d["stats"]["files"].as_u64().unwrap() as usize
            );
        }
        assert_eq!(data["files"][0]["additions"], 3);
        assert_eq!(data["files"][1]["additions"], 294);
    });
    run("test_01_ready_and_header_actions", || {
        p.commit("reference");
        assert_eq!(
            p.js("document.querySelector('.author').textContent"),
            data["author"]["name"]
        );
        assert_eq!(
            p.js("document.querySelector('.subject').textContent"),
            data["subject"]
        );
        p.check("document.querySelector('.date-line').title.includes('+05:30')");
        assert_eq!(p.js("document.querySelectorAll('.committer').length"), 0);
        assert_eq!(
            p.js("document.querySelector('.file-total').textContent"),
            "2 files changed"
        );
        p.js("document.getElementById('copy-sha').click()");
        assert_eq!(p.count("copy"), 1);
        p.check("document.getElementById('copy-sha').textContent.includes('Copied')");
        pump(1250);
        p.check("document.getElementById('copy-sha').textContent.includes('Commit SHA')");
        p.js("document.querySelector('.parent').click();document.querySelector('.ref').click();document.querySelector('.file-name').click()");
        assert_eq!(p.count("open-commit"), 2);
        let m = p.messages.borrow();
        assert!(m
            .iter()
            .any(|v| v["type"] == "open-file" && v["path"] == "README.md" && v["line"] == 249));
        p.shot("commit-unified-dark");
    });
    run("test_02_rows_syntax_wrap_and_selection", || {
        p.commit("reference");
        p.wait("document.querySelectorAll('.file[data-rendered=true]').length===2");
        p.check("document.querySelectorAll('.syntax').length>10");
        assert_eq!(p.js("document.querySelector('.added').dataset.new"), "249");
        assert_eq!(
            p.js("document.querySelector('.added .marker').textContent"),
            "+"
        );
        assert_eq!(
            p.js("getComputedStyle(document.querySelector('.file-header')).position"),
            "sticky"
        );
        assert_eq!(
            p.js("getComputedStyle(document.querySelector('.gutter')).webkitUserSelect"),
            "none"
        );
        assert_eq!(
            p.js("getComputedStyle(document.querySelector('.code')).whiteSpace"),
            "pre-wrap"
        );
        let copied=p.js("(()=>{const n=[...document.querySelectorAll('.added')].slice(0,3),r=document.createRange();r.setStartBefore(n[0]);r.setEndAfter(n[2]);const s=getSelection();s.removeAllRanges();s.addRange(r);const t=new DataTransfer();document.dispatchEvent(new ClipboardEvent('copy',{clipboardData:t,bubbles:true,cancelable:true}));s.removeAllRanges();return t.getData('text/plain')})()");
        assert!(copied.as_str().unwrap().contains("Drag between panes"));
        assert!(!copied.as_str().unwrap().contains("249"));
        p.window.resize(520, 900);
        p.web.set_size_request(520, 900);
        pump(100);
        p.check("document.documentElement.scrollWidth<=520");
        p.check("[...document.querySelectorAll('.code')].every(n=>n.scrollWidth<=n.clientWidth+1)");
        let light = get_theme(Some("impro-light"));
        p.call(
            "setTheme",
            &[commit_colors(light, &light.ui, &light.ui["accent"], 13)],
        );
        assert_eq!(
            p.js("getComputedStyle(document.body).backgroundColor"),
            "rgb(244, 247, 245)"
        );
        p.shot("commit-unified-light");
        p.web.set_size_request(900, 1000);
        p.window.resize(900, 1000);
        pump(100);
    });
    run("test_03_collapse_keyboard_and_split", || {
        p.commit("reference");
        p.js("document.querySelector('.file-header').dispatchEvent(new KeyboardEvent('keydown',{key:' ',bubbles:true,cancelable:true}))");
        assert_eq!(
            p.js("document.querySelector('.file-header').getAttribute('aria-expanded')"),
            "false"
        );
        p.js("document.getElementById('toggle-all').click()");
        assert_eq!(p.js("document.querySelectorAll('.collapsed').length"), 2);
        p.js("document.getElementById('toggle-all').click()");
        assert_eq!(p.js("document.querySelectorAll('.collapsed').length"), 0);
        p.js("document.querySelectorAll('.segment button')[1].click()");
        p.check("document.querySelectorAll('.split-pair').length>0");
        p.shot("commit-split-light");
        p.call(
            "setTheme",
            &[commit_colors(theme, &theme.ui, &theme.ui["accent"], 13)],
        );
        p.shot("commit-split-dark");
        p.js("document.querySelectorAll('.segment button')[0].click();document.querySelector('.file-header').focus();document.dispatchEvent(new KeyboardEvent('keydown',{key:'j',bubbles:true}))");
        p.check("document.activeElement.getAttribute('aria-label').includes('impro/panesplit.py')");
        p.js("document.dispatchEvent(new KeyboardEvent('keydown',{key:'k',bubbles:true}))");
        p.check("document.activeElement.getAttribute('aria-label').includes('README')");
    });
    run("test_04_context_roundtrip_and_offsets", || {
        p.commit("gaps");
        p.js("document.querySelector('.gap-label').click()");
        let req = request(&p);
        assert_eq!(req["start"], 1);
        assert_eq!(req["end"], 36);
        answer(&p, &req, false);
        assert_eq!(p.js("document.querySelectorAll('.gap-bar').length"), 2);
        assert_eq!(
            p.js("document.querySelector('.context-in').dataset.old"),
            "1"
        );
        p.js("document.querySelector('.gap-bar button[aria-label=\"Expand context below\"]').click()");
        let req = request(&p);
        assert_eq!(
            req["end"].as_i64().unwrap() - req["start"].as_i64().unwrap(),
            19
        );
        answer(&p, &req, false);
        assert_eq!(
            p.js(&format!(
                "document.querySelector('.context-in[data-new=\"{}\"]').dataset.old",
                req["start"]
            )),
            (req["start"].as_i64().unwrap() - 1).to_string()
        );
        p.js("document.querySelector('.gap-bar button[aria-label=\"Expand context above\"]').click()");
        let req = request(&p);
        p.js("document.querySelectorAll('.segment button')[1].click()");
        answer(&p, &req, false);
        assert_eq!(
            p.js(&format!(
                "document.querySelector('.context-in[data-new=\"{}\"]').dataset.old",
                req["start"]
            )),
            (req["start"].as_i64().unwrap() - 1).to_string()
        );
        p.js("document.querySelectorAll('.segment button')[0].click()");
        for _ in 0..10 {
            p.js("document.querySelector('.gap-bar .gap-label').click()");
            let req = request(&p);
            let empty = req["start"].as_i64().unwrap() > 200;
            answer(&p, &req, empty);
            if empty {
                break;
            }
        }
        assert_eq!(p.js("document.querySelectorAll('.gap-bar').length"), 0);
        p.check("(()=>{const n=[...document.querySelectorAll('.diff-row[data-new]')].map(n=>n.dataset.new).filter(Boolean);return n.length===new Set(n).size})()");
        p.js("document.querySelectorAll('.segment button')[1].click()");
        assert_eq!(p.js("document.querySelectorAll('.gap-bar').length"), 0);
        p.check("document.querySelectorAll('.split-pair').length>100");
        p.js("document.querySelectorAll('.segment button')[0].click()");
    });
    run("test_05_edge_files_and_long_unicode_lines", || {
        p.commit("edges");
        p.js("document.querySelectorAll('.file-header').forEach(n=>{if(n.getAttribute('aria-expanded')==='false')n.click();else{n.click();n.click();}})");
        assert_eq!(p.js("document.querySelectorAll('.file').length"), 7);
        for phrase in [
            "Binary file not shown",
            "Large diff not shown",
            "mode 100644 → 100755",
            "old.py → renamed.py",
            "No newline at end of file",
        ] {
            p.check(&format!(
                "document.body.textContent.includes({})",
                json!(phrase)
            ));
        }
        assert_eq!(
            p.js("document.querySelector('.file[data-status=deleted] .marker').textContent"),
            "-"
        );
        assert_eq!(p.js("document.querySelector('.file[data-status=deleted]').querySelectorAll('.gap').length"),0);
        p.check("document.querySelectorAll('.emphasis').length>0");
        p.check("document.querySelector('.file:last-child .code').textContent.includes('😀 café')");
    });
    run("test_06_merge_root_and_empty", || {
        p.commit("merge");
        assert_eq!(p.js("document.querySelectorAll('.parent').length"), 2);
        let d = fixture("gitcommit_merge.json");
        p.check(&format!(
            "document.querySelector('.banner').textContent.includes({})",
            json!(&d["diff_base"].as_str().unwrap()[..7])
        ));
        p.commit("root");
        p.check("document.body.textContent.includes('Root commit - showing the whole tree')");
        p.commit("empty");
        p.check("document.body.textContent.includes('This commit has no file changes.')");
    });
    run("test_07_states_and_reduced_motion", || {
        p.call("setState", &[json!("loading")]);
        assert_eq!(
            p.js("document.querySelectorAll('.skeleton-file').length"),
            2
        );
        p.call("setSettings", &[json!({"reduceMotion":true})]);
        assert_eq!(p.js("getComputedStyle(document.querySelector('.skeleton-line'),'::after').animationName"),"none");
        p.call(
            "setState",
            &[json!("error"), json!("Git could not read this repository.")],
        );
        p.check("document.body.textContent.includes('Git could not read')&&document.body.textContent.includes('Refresh')");
        p.call("setState", &[json!("not-found")]);
        p.check("document.body.textContent.includes('Commit not found')");
        p.commit("reference");
        assert_eq!(
            p.js("getComputedStyle(document.querySelector('.file-body')).transitionDuration"),
            "0s"
        );
        assert_eq!(
            p.js("getComputedStyle(document.querySelector('.file')).animationName"),
            "none"
        );
        p.call("pause", &[]);
        p.check("document.documentElement.classList.contains('paused')");
        p.call("resume", &[]);
        p.check("!document.documentElement.classList.contains('paused')");
    });
    run("test_08_standalone_and_ready_once", || {
        let q = Page::new(true, false);
        q.commit("reference");
        q.js("document.getElementById('copy-sha').click();document.querySelector('.gap-label').click()");
        q.check("document.body.textContent.includes('Open in impro')");
        assert_eq!(q.messages.borrow().len(), 0);
        assert_eq!(q.js("document.querySelectorAll('script[nonce]').length"), 2);
        assert_eq!(p.count("ready"), 1);
    });
    run("test_09_lazy_render_and_performance", || {
        p.js("window.__start=performance.now()");
        p.call("setCommit", &[generated(50)]);
        p.js("requestAnimationFrame(()=>requestAnimationFrame(()=>window.__paint=performance.now()-window.__start))");
        p.wait("window.__paint>0");
        let ms = p.js("window.__paint").as_f64().unwrap();
        println!("50-files-first-paint-ms={ms:.2}");
        assert!(ms < 1000.);
        assert_eq!(p.js("document.querySelectorAll('.collapsed').length"), 50);
        assert_eq!(
            p.js("document.querySelectorAll('[data-rendered=true]').length"),
            0
        );
        p.js("document.querySelector('.file-header').click()");
        assert_eq!(
            p.js("document.querySelectorAll('[data-rendered=true]').length"),
            1
        );
        p.call("setCommit", &[generated(300)]);
        assert_eq!(p.js("document.querySelectorAll('.file').length"), 300);
        p.call("setCommit", &[generated(30)]);
        pump(300);
        let n = p
            .js("document.querySelectorAll('[data-rendered=true]').length")
            .as_u64()
            .unwrap();
        assert!(n > 0 && n < 8);
        p.js("document.querySelector('.file:last-child').scrollIntoView()");
        p.wait("document.querySelector('.file:last-child').dataset.rendered==='true'");
    });
    run("test_10_theme_contrast_and_syntax_offsets", || {
        let mut d = data.clone();
        d["files"] = json!([d["files"][1]]);
        d["files"][0]["hunks"][0]["lines"] = json!([{"t":"+","o":null,"n":1,"text":"😀 keyword café","k":[[2,9,"keyword"]]},{"t":"-","o":1,"n":null,"text":"😀 comment café","k":[[2,9,"comment"]]}]);
        p.call("setCommit", &[d]);
        assert_eq!(
            p.js("document.querySelector('.syntax').textContent"),
            "keyword"
        );
        let mut colors = commit_colors(theme, &theme.ui, &theme.ui["accent"], 13);
        colors["syntax"] = json!({});
        p.call("setTheme", &[colors]);
        assert_eq!(
            p.js("getComputedStyle(document.querySelector('.syntax')).color"),
            p.js("getComputedStyle(document.querySelector('.code')).color")
        );
    });
    assert_eq!(p.count("error"), 0, "{:?}", p.messages.borrow());
}
