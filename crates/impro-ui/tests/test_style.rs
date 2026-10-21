mod common;
use common::*;
use gtk::prelude::*;
use impro_core::themes::*;
use impro_ui::style::*;
#[test]
fn test_every_theme_parses() {
    run(|| {
        impro_ui::init();
        for t in themes() {
            let css = build_css(t);
            create_provider(&css).unwrap();
            assert!(css.contains(".impro-resize-gate"));
            assert!(css.contains("impro-note-toolbar"));
            assert!(!css.contains("{ui["));
        }
    });
}
#[test]
fn test_impro_geometry_and_css_tokens() {
    assert_eq!(TOKENS["spacing"]["tight"], 3);
    assert_eq!(TOKENS["radius"]["button"], 7);
    let css = build_css(get_theme(None));
    assert!(css.contains("font-size: 14px"));
    assert!(css.contains("border-radius: 11px"));
    assert!(css.contains("0 5px 17px alpha(#11191d, 0.38)"));
    assert!(build_css(get_theme(Some("impro-light"))).contains("0 5px 17px alpha(#11191d, 0.16)"));
}
#[test]
fn test_base_rules_and_instant_states() {
    let css = build_css(get_theme(None));
    assert!(css.contains("border-color 128ms"));
    assert!(css.contains("background-color 128ms"));
    assert!(css.contains("transition: none"));
    assert!(!css.contains(":focus-visible"));
}
#[test]
fn test_live_switch_uses_one_provider_and_updates_widgets() {
    run(|| {
        impro_ui::init();
        apply_style(None).unwrap();
        let initial = provider().unwrap();
        let label = gtk::Label::new(Some("hello"));
        let window = window(&label);
        for id in ["impro-light", "nord", "impro-dark"] {
            apply_style(Some(get_theme(Some(id)))).unwrap();
            assert_eq!(initial, provider().unwrap());
            assert_eq!(current_theme().unwrap().id, id);
            pump(20);
        }
        unsafe {
            window.destroy();
        }
    });
}
#[test]
fn test_invalid_css_raises_clear_exception() {
    run(|| {
        let e = create_provider("label { made-up-property: invalid; }").unwrap_err();
        assert!(e.to_string().contains("impro theme CSS failed to parse"));
    });
}
#[test]
fn test_css_registry_and_fstring_templates() {
    run(|| {
        register_css("test.label", |ui| {
            render_css(".test {{ color: {ui['accent']}; background: {bg}; }}", ui)
        });
        let theme = get_theme(None);
        let css = build_css(theme);
        assert!(css.contains(&format!(
            ".test {{ color: {}; background: {}; }}",
            theme.ui["accent"], theme.ui["bg"]
        )));
        create_provider(&css).unwrap();
    });
}
#[test]
fn test_font_overrides_and_accent() {
    for t in themes() {
        let changed = with_accent(t, Some("#3B82F6"));
        assert_eq!(changed.ui["accent"], "#3b82f6");
        assert!(contrast(&changed.ui["accent_fg"], &changed.ui["accent"]) >= 4.5);
        assert!(contrast(&changed.ui["accent_text"], &changed.ui["bg"]) >= 3.0);
    }
    let css = build_overrides(Some("Inter\";{}\\"), Some("Fira Code"));
    assert!(css.contains("font-family: \"Inter\""));
    assert!(css.contains("font-family: \"Fira Code\""));
    assert_eq!(with_accent(get_theme(None), None), *get_theme(None));
}
#[test]
fn test_non_overlay_scrollbars_are_ten_pixels() {
    run(|| {
        impro_ui::init();
        apply_style(None).unwrap();
        let scroll = gtk::ScrolledWindow::new(gtk::Adjustment::NONE, gtk::Adjustment::NONE);
        scroll.set_overlay_scrolling(false);
        let content = gtk::Label::new(Some("Scrollable content"));
        content.set_size_request(1000, 1000);
        scroll.add(&content);
        let window = window(&scroll);
        pump(50);
        assert_eq!(scroll.vscrollbar().unwrap().allocated_width(), 10);
        assert_eq!(scroll.hscrollbar().unwrap().allocated_height(), 10);
        unsafe {
            window.destroy();
        }
    });
}
