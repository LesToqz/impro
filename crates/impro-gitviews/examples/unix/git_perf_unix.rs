//! Criterion-free real-process load/paint/idle/lifecycle measurements.
#[path = "../../tests/common/mod.rs"]
mod common;
#[path = "../../tests/support/mod.rs"]
mod support;
use common::*;
use gtk::prelude::*;
use impro_gitviews::{CommitView, GitGraphView};
use std::time::Instant;
use support::*;
fn processes() -> Vec<(u32, u64, u64)> {
    let mut out = Vec::new();
    for entry in std::fs::read_dir("/proc").unwrap().flatten() {
        let Ok(pid) = entry.file_name().to_string_lossy().parse::<u32>() else {
            continue;
        };
        let Ok(stat) = std::fs::read_to_string(entry.path().join("stat")) else {
            continue;
        };
        let Some(end) = stat.rfind(')') else {
            continue;
        };
        if !stat[..end].contains("WebKitWeb") {
            continue;
        }
        let f: Vec<_> = stat[end + 2..].split_whitespace().collect();
        if f.get(1).and_then(|s| s.parse::<u32>().ok()) != Some(std::process::id()) {
            continue;
        }
        let ticks = f[11].parse::<u64>().unwrap() + f[12].parse::<u64>().unwrap();
        let rss = std::fs::read_to_string(entry.path().join("status"))
            .ok()
            .and_then(|s| {
                s.lines()
                    .find(|s| s.starts_with("VmRSS:"))
                    .map(str::to_owned)
            })
            .and_then(|s| s.split_whitespace().nth(1).and_then(|s| s.parse().ok()))
            .unwrap_or(0);
        out.push((pid, ticks, rss));
    }
    out
}
pub(super) fn main() {
    gtk::init().expect("GTK display");
    impro_gitviews::init();
    assert!(processes().is_empty());
    println!("web-processes-before-first-page=0");
    let temp = Temp::new();
    let repo = GitRepo::new(&temp.0);
    let mut stream = String::new();
    for n in 0..1500 {
        let subject = format!("History commit {n}\n");
        let text = format!("pub fn number() -> u32 {{ {n} }}\n");
        stream.push_str(&format!("commit refs/heads/master\nmark :{}\nauthor Ada Example <ada@example.test> {} +0000\ncommitter Ada Example <ada@example.test> {} +0000\ndata {}\n{}",n+1,1700000000+n,1700000000+n,subject.len(),subject));
        if n > 0 {
            stream.push_str(&format!("from :{n}\n"));
        }
        stream.push_str(&format!(
            "M 100644 inline main.rs\ndata {}\n{}\n",
            text.len(),
            text
        ));
    }
    stream.push_str("done\n");
    repo.command(&["fast-import", "--quiet"], Some(stream.as_bytes()), &[]);
    let sha = repo.git(&["rev-parse", "HEAD"]);
    let start = Instant::now();
    let data = impro_gitviews::log::collect_graph(&repo.path, Some(1500));
    let ms = start.elapsed().as_secs_f64() * 1000.;
    assert_eq!(data["commits"].as_array().unwrap().len(), 1500);
    println!("collect-1500-ms={ms:.3}");
    {
        let first = CommitView::new(&repo.path, &sha, None);
        let first_window = gtk::Window::new(gtk::WindowType::Toplevel);
        first_window.set_default_size(900, 900);
        first_window.add(first.widget());
        first_window.show_all();
        poll(|| first.timings().data_to_paint.is_some());
        let timings = first.timings();
        let ms = timings.data_to_paint.unwrap().as_secs_f64() * 1000.;
        println!(
            "commit-first-ever-open-to-paint-ms={:.3}; data-to-paint-ms={ms:.3}",
            timings.open_to_first_paint.unwrap().as_secs_f64() * 1000.
        );
        assert!(ms < 150., "first-ever commit paint budget");
        first.shutdown();
        unsafe {
            first_window.destroy();
        }
        poll(|| processes().is_empty());
    }
    let graph = GitGraphView::new(&repo.path);
    graph.set_reduce_motion(true);
    let w = gtk::Window::new(gtk::WindowType::Toplevel);
    w.set_default_size(1100, 520);
    w.add(graph.widget());
    w.show_all();
    poll(|| graph.timings().open_to_first_paint.is_some());
    println!(
        "graph-open-to-first-paint-ms={:.3}",
        graph.timings().open_to_first_paint.unwrap().as_secs_f64() * 1000.
    );
    println!(
        "graph-data-to-paint-ms={:.3}",
        graph.timings().data_to_paint.unwrap().as_secs_f64() * 1000.
    );
    let detail = CommitView::new(&repo.path, &sha, None);
    let cw = gtk::Window::new(gtk::WindowType::Toplevel);
    cw.set_default_size(900, 900);
    cw.add(detail.widget());
    cw.show_all();
    poll(|| detail.timings().data_to_paint.is_some());
    let cold = detail.timings();
    println!(
        "commit-cold-open-to-first-paint-ms={:.3}",
        cold.open_to_first_paint.unwrap().as_secs_f64() * 1000.
    );
    println!(
        "commit-cold-data-to-paint-ms={:.3}",
        cold.data_to_paint.unwrap().as_secs_f64() * 1000.
    );
    let sibling = CommitView::new(&repo.path, &sha, None);
    let sw = gtk::Window::new(gtk::WindowType::Toplevel);
    sw.set_default_size(900, 900);
    sw.add(sibling.widget());
    sw.show_all();
    poll(|| sibling.timings().data_to_paint.is_some());
    let warm = sibling.timings().data_to_paint.unwrap().as_secs_f64() * 1000.;
    println!("commit-shared-data-to-paint-ms={warm:.3}");
    assert!(warm < 150., "commit paint budget");
    graph.widget().hide();
    detail.widget().hide();
    sibling.widget().hide();
    pump(8000);
    let before = processes();
    let start = Instant::now();
    pump(3000);
    let after = processes();
    let ticks_before: u64 = before.iter().map(|p| p.1).sum();
    let ticks_after: u64 = after.iter().map(|p| p.1).sum();
    let ticks = ticks_after.saturating_sub(ticks_before);
    let hz = std::process::Command::new("getconf")
        .arg("CLK_TCK")
        .output()
        .unwrap();
    let hz = String::from_utf8_lossy(&hz.stdout)
        .trim()
        .parse::<f64>()
        .unwrap();
    let cpu = ticks as f64 / hz / start.elapsed().as_secs_f64() * 100.;
    println!(
        "hidden-web-idle-cpu-percent={cpu:.3}; ticks={ticks}; web-processes={}; rss-kib={:?}",
        after.len(),
        after.iter().map(|p| p.2).collect::<Vec<_>>()
    );
    graph.shutdown();
    detail.shutdown();
    assert_eq!(js(sibling.webview(), "!!window.ImproCommit"), true);
    sibling.shutdown();
    unsafe {
        w.destroy();
        cw.destroy();
        sw.destroy();
    }
    poll(|| processes().is_empty());
    println!("web-processes-after-shutdown=0");
}
