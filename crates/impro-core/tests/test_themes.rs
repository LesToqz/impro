use impro_core::themes::*;
#[test]
fn test_catalogue_and_lookup() {
    assert_eq!(
        theme_ids(),
        [
            "dark-plus",
            "light-plus",
            "monokai",
            "dracula",
            "solarized-dark",
            "solarized-light",
            "nord",
            "one-dark",
            "github-dark",
            "impro-midnight",
            "impro-ember",
            "impro-paper",
            "impro-dark",
            "impro-light"
        ]
    );
    assert_eq!(get_theme(None).id, DEFAULT_THEME_ID);
    assert_eq!(get_theme(Some("missing")).id, DEFAULT_THEME_ID);
}
#[test]
fn test_complete_and_valid_colors() {
    let roles: Vec<_> = themes()[0].ui.keys().collect();
    for t in themes() {
        assert_eq!(t.ui.keys().collect::<Vec<_>>(), roles);
        assert_eq!(t.syntax.len(), KINDS.len());
        for k in KINDS {
            assert!(t.syntax.contains_key(k));
        }
        assert_eq!(t.chrome.len(), 6);
        for c in
            t.ui.values()
                .chain(t.chrome.values())
                .chain(t.terminal.ansi.iter())
                .chain(t.syntax.values().map(|s| &s.foreground))
        {
            assert!(valid_color(c), "{} {c}", t.id);
        }
        assert_eq!(t.dark, luminance(&t.ui["bg"]) < 0.3);
    }
}
#[test]
fn test_contrast() {
    for t in themes() {
        for (a, b, min) in [
            ("editor_fg", "bg", 7.0),
            ("foreground", "bg", 4.5),
            ("foreground", "sidebar_bg", 4.5),
            ("muted", "sidebar_bg", 3.0),
            ("accent_fg", "accent", 4.5),
        ] {
            assert!(contrast(&t.ui[a], &t.ui[b]) >= min, "{} {a}", t.id);
        }
        for (k, s) in &t.syntax {
            assert!(
                contrast(&s.foreground, &t.ui["bg"]) >= if k == "comment" { 2.2 } else { 3.0 },
                "{} {k}",
                t.id
            );
        }
        assert!(contrast(&t.terminal.foreground, &t.terminal.background) >= 7.0);
    }
}
#[test]
fn test_git_colors_are_readable_on_sidebar_and_selected_rows() {
    for t in themes() {
        let selected = mix_color(&t.ui["sidebar_bg"], &t.ui["overlay"], 0.09);
        for r in ["git_new", "git_modified", "git_conflict"] {
            assert!(contrast(&t.ui[r], &t.ui["sidebar_bg"]) >= 4.5);
            assert!(contrast(&t.ui[r], &selected) >= 4.5);
        }
    }
}
#[test]
fn test_terminal_palette_rejects_invalid_ansi() {
    for colors in [
        vec!["#ffffff".into(); 15],
        vec!["#ffffff".into(); 17],
        vec!["#GGGGGG".into(); 16],
        vec!["#fff".into(); 16],
    ] {
        assert!(
            TerminalColors::new("#ffffff".into(), "#000000".into(), "#ffffff".into(), colors)
                .is_err()
        );
    }
}
#[test]
fn test_impro_mineral_palettes() {
    for (id, bg, accent) in [
        ("impro-dark", "#182126", "#72b9ad"),
        ("impro-light", "#f4f7f5", "#356e60"),
    ] {
        let t = get_theme(Some(id));
        assert_eq!(t.ui["bg"], bg);
        assert_eq!(t.ui["accent"], accent);
        assert_eq!(t.ui["status_bg"], t.ui["sidebar_bg"]);
    }
}
#[test]
fn test_dark_plus_matches_original() {
    let t = get_theme(Some("dark-plus"));
    assert_eq!(t.ui["bg"], "#1e1e1e");
    assert_eq!(t.syntax["comment"].foreground, "#6A9955");
    assert_eq!(t.terminal.ansi[1], "#cd3131");
    assert_eq!(t.chrome["bracket_bg"], "#3a3d41");
}
#[test]
fn test_contrast_helpers() {
    assert_eq!(contrast("#ffffff", "#000000"), 21.0);
    assert_eq!(mix_color("#000000", "#ffffff", 0.5), "#808080");
    assert!(contrast(&legible("#444444", "#333333", 7.0), "#333333") >= 7.0);
}
