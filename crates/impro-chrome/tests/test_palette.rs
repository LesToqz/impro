use impro_chrome::{
    fuzzy::{self, Range},
    *,
};
fn m(q: &str, t: &str) -> (f64, Vec<Range>) {
    fuzzy::r#match(q, t, false).expect("match")
}
fn labels(q: &str, values: &[&str], path: bool) -> Vec<String> {
    rank(
        q,
        &values.iter().map(|s| s.to_string()).collect::<Vec<_>>(),
        path,
        Some(MAX_RESULTS),
    )
    .into_iter()
    .map(|v| v.0)
    .collect()
}
#[test]
fn test_empty_query_matches_everything_without_ranges() {
    assert_eq!(m("", "anything"), (0., vec![]));
    assert_eq!(m("   ", "anything"), (0., vec![]));
    assert_eq!(labels("", &["b", "a"], false), ["b", "a"]);
}
#[test]
fn test_order_matters_and_missing_characters_fail() {
    assert!(fuzzy::r#match("lpa", "Plan.md", false).is_none());
    assert!(fuzzy::r#match("plx", "Plan.md", false).is_none());
    assert!(fuzzy::r#match("pmd", "Plan.md", false).is_some());
}
#[test]
fn test_pla_ranks_plan_md_first_and_file_names_outrank_folders() {
    let r = labels(
        "pla",
        &[
            "notes/plan/todo.md",
            "explain.md",
            "apple.md",
            "people/lamp.md",
            "Planning/Plan.md",
            "p_l_a.md",
            "Plan.md",
        ],
        true,
    );
    assert_eq!(r[0], "Plan.md");
    assert!(!r.contains(&"apple.md".into()));
    let i = |v| r.iter().position(|s| s == v).unwrap();
    assert!(i("Planning/Plan.md") < i("p_l_a.md"));
    assert!(i("explain.md") < i("notes/plan/todo.md"));
    assert_eq!(
        fuzzy::r#match("pla", "Planning/Plan.md", true).unwrap().1,
        [(9, 12)]
    );
    assert_eq!(fuzzy::r#match("pla", "Plan.md", true).unwrap().1, [(0, 3)]);
}
#[test]
fn test_word_boundaries_win() {
    assert_eq!(
        labels("fb", &["fabric.py", "foo_bar.py"], false),
        ["foo_bar.py", "fabric.py"]
    );
    assert_eq!(m("fb", "foo_bar.py").1, [(0, 1), (4, 5)]);
    assert_eq!(
        labels("gfn", &["gifnorm", "getFileName"], false),
        ["getFileName", "gifnorm"]
    );
    assert_eq!(m("gfn", "getFileName").1, [(0, 1), (3, 4), (7, 8)]);
    assert_eq!(m("v2", "view2").1, [(0, 1), (4, 5)]);
}
#[test]
fn test_consecutive_and_compact_matches_win() {
    assert_eq!(
        labels("main", &["m_a_i_n.py", "mxaxixn.py", "main.py"], false),
        ["main.py", "m_a_i_n.py", "mxaxixn.py"]
    );
    assert!(m("ab", &format!("a{}b", "x".repeat(30))).0 < m("ab", "axb").0);
}
#[test]
fn test_late_first_match_and_long_candidates_are_penalised() {
    assert!(m("x", &format!("x{}", "a".repeat(20))).0 > m("x", &format!("{}x", "a".repeat(20))).0);
    assert!(m("abc", "abc").0 > m("abc", &format!("abc{}", "z".repeat(60))).0);
}
#[test]
fn test_tokens_must_all_match_in_order() {
    assert_eq!(
        fuzzy::r#match("src main", "src/app/main.py", true)
            .unwrap()
            .1,
        [(0, 3), (8, 12)]
    );
    assert!(fuzzy::r#match("main zzz", "src/app/main.py", false).is_none());
    assert!(fuzzy::r#match("main src", "src/app/main.py", false).is_none());
}
#[test]
fn test_case_rules() {
    assert!(fuzzy::r#match("PLAN", "plan.md", false).is_some());
    assert!(fuzzy::r#match("plan", "PLAN.MD", false).is_some());
    assert_eq!(
        labels("readme", &["README.md", "readme.md"], false),
        ["README.md", "readme.md"]
    );
    assert_eq!(
        labels("readme", &["readme.md", "README.md"], false),
        ["readme.md", "README.md"]
    );
    assert_eq!(
        labels("README", &["readme.md", "README.md"], false),
        ["README.md", "readme.md"]
    );
}
#[test]
fn test_unicode() {
    assert_eq!(m("ä", "Ärger.md").1, [(0, 1)]);
    assert_eq!(m("ÄR", "märz ärger").1, [(5, 7)]);
    assert_eq!(
        fuzzy::r#match("日本", "資料/日本語.md", true).unwrap().1,
        [(3, 5)]
    );
    assert_eq!(m("🙂x", "a🙂x").1, [(1, 3)]);
    assert_eq!(m("x", "İİx").1, [(2, 3)]);
    assert_eq!(m("İ", "xİy").1, [(1, 2)]);
}
#[test]
fn test_ties_keep_source_order() {
    let mut i = FuzzyIndex::new((0..5).collect(), |_| "same".into(), false);
    assert_eq!(
        i.search("sa", Some(100))
            .into_iter()
            .map(|v| v.0)
            .collect::<Vec<_>>(),
        [0, 1, 2, 3, 4]
    );
}
#[test]
fn test_limit_and_key() {
    let mut i = FuzzyIndex::new((0..500).collect(), |n| format!("file{n}.txt"), false);
    assert_eq!(i.search("file", Some(MAX_RESULTS)).len(), 100);
    assert_eq!(i.search("file", None).len(), 500);
    assert_eq!(i.search("file", Some(3)).len(), 3);
    assert!(i.search("file", Some(0)).is_empty());
}
fn paths(n: usize) -> Vec<String> {
    (0..n)
        .map(|i| {
            format!(
                "{}/{}/{}_{}{}.{}",
                ["src", "lib", "test", "docs"][i % 4],
                ["core", "view", "plan", "utils"][i % 4],
                ["main", "palette", "plan", "editor"][i % 4],
                "helper",
                i,
                ["py", "md", "js", "txt"][i % 4]
            )
        })
        .collect()
}
#[test]
fn test_incremental_index_matches_fresh_ranking() {
    let paths = paths(3000);
    let mut i = FuzzyIndex::new(paths.clone(), Clone::clone, true);
    for q in [
        "p", "pl", "pla", "plan", "plan ", "plan m", "plan md", "plan", "x", "xy", "",
    ] {
        assert_eq!(
            i.search(q, Some(100)),
            rank(q, &paths, true, Some(100)),
            "{q}"
        );
    }
}
#[test]
fn test_highlight_markup_escapes_and_bolds() {
    let markup = highlight_markup("a<b&c>", &[(1, 3)]);
    assert_eq!(markup, "a<b>&lt;b</b>&amp;c&gt;");
    let (_, text, _) = pango::parse_markup(&markup, '\0').unwrap();
    assert_eq!(text, "a<b&c>");
    assert_eq!(
        highlight_markup("abc", &[(2, 9), (0, 1)]),
        "<b>a</b>b<b>c</b>"
    );
    assert_eq!(highlight_markup("x & y", &[]), "x &amp; y");
}
#[test]
fn test_split_path_ranges() {
    let path = "src/plan/planner.py";
    let ranges = fuzzy::r#match("plan pla", path, true).unwrap().1;
    assert_eq!(
        split_path_ranges(path, &ranges),
        (vec![(4, 8)], vec![(0, 3)])
    );
    assert_eq!(split_path_ranges("a.py", &[(0, 1)]), (vec![], vec![(0, 1)]));
}
#[test]
fn test_prompt_geometry() {
    assert_eq!(prompt_geometry(1000, 800), (150, 120, 700, 560));
    assert_eq!(prompt_geometry(600, 400), (60, 60, 480, 280));
}
#[test]
fn test_twenty_thousand_items_filter_quickly() {
    let mut index = FuzzyIndex::new(paths(20000), Clone::clone, true);
    for q in [
        "e", "p", "pl", "pla", "plan", "plan m", "zzz", "src/main", "pal th",
    ] {
        let start = std::time::Instant::now();
        assert!(index.search(q, Some(100)).len() <= 100);
        assert!(start.elapsed().as_secs_f64() < 1.);
    }
}
#[test]
#[ignore]
fn benchmark_rank() {
    let paths = paths(20000);
    let mut index = FuzzyIndex::new(paths, Clone::clone, true);
    for q in ["e", "src main", "pal th"] {
        let start = std::time::Instant::now();
        let result = index.search(q, Some(100));
        println!(
            "rank 20k {q:?}: {:.3} ms, {} rows",
            start.elapsed().as_secs_f64() * 1000.,
            result.len()
        );
    }
}
