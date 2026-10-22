use impro_core::themes::{self, contrast, KINDS};
use impro_gitviews::{
    palette::{commit_colors, git_colors},
    GraphSettings,
};
use serde_json::json;

#[test]
fn test_graph_palette_and_commit_roles() {
    for theme in themes::themes() {
        for accent in [&theme.ui["accent"], "#5588ee"] {
            let c = git_colors(&theme.ui, accent);
            let colors = c["branch_colors"].as_array().unwrap();
            assert_eq!(colors.len(), 8);
            let unique: std::collections::HashSet<_> =
                colors.iter().map(|v| v.as_str().unwrap()).collect();
            assert_eq!(unique.len(), 8);
            for color in colors {
                let rgb = |s: &str| {
                    let n = u32::from_str_radix(s.trim_start_matches('#'), 16).unwrap();
                    [
                        ((n >> 16) & 255) as f64 / 255.,
                        ((n >> 8) & 255) as f64 / 255.,
                        (n & 255) as f64 / 255.,
                    ]
                };
                let distance = rgb(color.as_str().unwrap())
                    .into_iter()
                    .zip(rgb(accent))
                    .map(|(a, b)| (a - b).powi(2))
                    .sum::<f64>()
                    .sqrt();
                assert!(distance >= 0.22);
                assert!(contrast(color.as_str().unwrap(), c["background"].as_str().unwrap()) >= 3.);
            }
            assert_eq!(c.as_object().unwrap().len(), 13);
        }
        let c = commit_colors(theme, &theme.ui, &theme.ui["accent"], 13);
        assert_eq!(c["syntax"].as_object().unwrap().len(), KINDS.len());
        for (kind, style) in &theme.syntax {
            assert_eq!(c["syntax"][kind], style.foreground);
        }
        assert_eq!(c["add"], theme.ui["git_new"]);
        assert!(c["add_bg"].as_str().unwrap().starts_with("rgba("));
        assert_eq!(c["dark"], theme.dark);
    }
}
#[test]
fn test_fonts_and_missing_webkit() {
    let mut ui = themes::get_theme(None).ui.clone();
    ui.insert("ui_font".into(), "Test UI".into());
    ui.insert("editor_font".into(), "Code Mono".into());
    let c = git_colors(&ui, "#007acc");
    assert_eq!(c["mono_font"], "\"Code Mono\", monospace");
    assert_eq!(c["ui_font"], "\"Test UI\", system-ui, sans-serif");
    assert_eq!(
        GraphSettings::from_value(&json!({})),
        GraphSettings::default()
    );
    let s = GraphSettings::from_value(
        &json!({"gitgraph_orientation":"vertical","gitgraph_max_commits":20000,"gitgraph_show_tags":false,"gitgraph_time_format":"absolute"}),
    );
    assert_eq!(s.max_commits, 10000);
    assert_eq!(s.orientation, "vertical");
    assert!(!s.show_tags);
    assert_eq!(s.time_format, "absolute");
    assert_eq!(GraphSettings::from_value(&s.to_value()), s);
}
#[test]
fn test_real_webkit_hosts_and_pages_in_main_thread_process() {
    let exe = std::env::current_exe()
        .unwrap()
        .parent()
        .unwrap()
        .parent()
        .unwrap()
        .join("examples/webkit_host_qa");
    let output = std::process::Command::new(exe)
        .output()
        .expect("cargo test builds the QA example");
    assert!(
        output.status.success(),
        "{}\n{}",
        String::from_utf8_lossy(&output.stdout),
        String::from_utf8_lossy(&output.stderr)
    );
    println!("{}", String::from_utf8_lossy(&output.stdout));
}

fn core_script(name: &str) {
    let path = std::path::Path::new(env!("CARGO_MANIFEST_DIR"))
        .join("tests/js")
        .join(name);
    let output = std::process::Command::new("node")
        .arg("--test")
        .arg(path)
        .output()
        .expect("Node for page core tests");
    assert!(
        output.status.success(),
        "{}\n{}",
        String::from_utf8_lossy(&output.stdout),
        String::from_utf8_lossy(&output.stderr)
    );
}
#[test]
fn test_gitgraph_core() {
    core_script("gitgraph_core.test.mjs");
}
#[test]
fn test_pure_helpers() {
    core_script("gitcommit_core.test.mjs");
}
