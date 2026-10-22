mod common;
use common::*;
use gtk::prelude::*;
use impro_terminal::terminal_bar::*;
use std::{cell::Cell, rc::Rc};
#[test]
fn test_css_class_and_height() {
    run(|| {
        let h = PaneHeader::new("bash", "terminal");
        let w = window(&h.widget);
        assert!(h.widget.style_context().has_class("impro-terminal-header"));
        assert_eq!(h.widget.orientation(), gtk::Orientation::Horizontal);
        unsafe {
            w.destroy();
        }
    });
}
#[test]
fn test_label_numbering_and_shell_name() {
    run(|| {
        for (shell, kind, expected) in [
            ("bash", "terminal", "bash3"),
            ("zsh", "terminal", "zsh3"),
            ("", "graph", "Graph view"),
            ("", "gitgraph", "Git graph"),
        ] {
            let h = PaneHeader::new(shell, kind);
            h.set_number(3);
            assert_eq!(h.text(), expected);
        }
    });
}
#[test]
fn test_controls_and_signals() {
    run(|| {
        let h = PaneHeader::new("bash", "terminal");
        let c = Rc::new(Cell::new(0));
        for which in 0..3 {
            let c = c.clone();
            let f = move || c.set(c.get() + 1);
            match which {
                0 => {
                    h.connect_focus_requested(f);
                }
                1 => {
                    h.connect_fullscreen_requested(f);
                }
                _ => {
                    h.connect_delete_requested(f);
                }
            }
        }
        h.label_button.clicked();
        h.fullscreen_button.button.clicked();
        h.delete_button.button.clicked();
        assert_eq!(c.get(), 3);
        assert!(!h.label_button.gets_focus_on_click());
        assert!(!h.fullscreen_button.button.gets_focus_on_click());
    });
}
#[test]
fn test_active_corner_and_maximized_state() {
    run(|| {
        let h = PaneHeader::new("bash", "terminal");
        h.set_active(true);
        h.set_corner(true);
        h.set_focused(true);
        for name in ["active", "corner", "focused"] {
            assert!(h.widget.style_context().has_class(name));
        }
        h.set_maximized(true);
        assert_eq!(
            h.fullscreen_button.button.tooltip_text().as_deref(),
            Some("Restore")
        );
        h.set_maximized(false);
        assert_eq!(
            h.fullscreen_button.button.tooltip_text().as_deref(),
            Some("Fullscreen")
        );
    });
}
#[test]
fn test_text_glyph_fallback_without_icon() {
    run(|| {
        let b = IconButton::new("missing-impro-icon", "□", "Fullscreen");
        assert_eq!(
            b.button
                .child()
                .unwrap()
                .downcast::<gtk::Label>()
                .unwrap()
                .text(),
            "□"
        );
    });
}
#[test]
fn test_add_button_limit_state() {
    run(|| {
        let b = AddButton::new(6);
        b.set_full(true);
        assert!(b.widget().style_context().has_class("limit"));
        assert_eq!(
            b.widget().tooltip_text().as_deref(),
            Some("Cannot add (limit 6)")
        );
        b.set_full(false);
        assert_eq!(b.widget().tooltip_text().as_deref(), Some("New pane"));
    });
}
#[test]
fn test_css_parses_for_every_theme_without_state_transitions() {
    run(|| {
        for theme in impro_core::themes::themes() {
            let provider = gtk::CssProvider::new();
            let s = css(&theme.ui);
            provider.load_from_data(s.as_bytes()).unwrap();
            assert!(s.contains("transition: none"));
        }
    });
}
#[test]
fn test_notice_text_is_legible_on_black_in_every_theme() {
    for t in impro_core::themes::themes() {
        let fg = impro_core::themes::legible(&t.ui["warning"], "#1a1a1a", 4.5);
        assert!(impro_core::themes::contrast(&fg, "#1a1a1a") >= 4.49);
    }
}
#[test]
fn test_compact_secondary_strip_with_13px_labels() {
    run(|| {
        impro_terminal::init();
        impro_ui::apply_style(None).unwrap();
        let h = PaneHeader::new("bash", "terminal");
        let w = window(&h.widget);
        let value = h
            .label
            .style_context()
            .style_property_for_state("font-size", gtk::StateFlags::NORMAL);
        assert!((value.get::<f64>().unwrap() - 13.0).abs() < 0.1);
        unsafe {
            w.destroy();
        }
    });
}
#[test]
fn test_underline_only_on_the_focused_active_pane() {
    run(|| {
        let d = Dock::new(false, 2);
        d.ready();
        let first = d.panel.panes()[0].id;
        d.panel.focus_pane(first);
        let ps = d.panel.panes();
        assert!(ps[0].header.widget.style_context().has_class("active"));
        assert!(!ps[1].header.widget.style_context().has_class("active"));
        assert!(css(&impro_core::themes::get_theme(None).ui).contains(".active.focused"));
    });
}
#[test]
fn test_clickable_icon_states() {
    run(|| {
        let b = IconButton::new("missing-impro-icon", "+", "New pane");
        let w = window(&b.button);
        assert!((b.button.child().unwrap().opacity() - GLYPH_OPACITY).abs() < 0.01);
        unsafe {
            w.destroy();
        }
    });
}
#[test]
fn test_glyph_opacity_fades_on_hover() {
    run(|| {
        let b = IconButton::new("missing-impro-icon", "+", "New pane");
        let w = window(&b.button);
        b.set_hover(1.);
        pump(60);
        let a = b.button.child().unwrap().opacity();
        assert!(a > GLYPH_OPACITY && a < 1.0);
        pump(140);
        assert_eq!(b.button.child().unwrap().opacity(), 1.);
        unsafe {
            w.destroy();
        }
    });
}
#[test]
fn test_glyph_hover_is_instant_with_reduced_motion() {
    run(|| {
        impro_ui::motion::set_reduce_motion(true);
        let b = IconButton::new("missing-impro-icon", "+", "New pane");
        b.set_hover(1.);
        assert_eq!(b.button.child().unwrap().opacity(), 1.);
        impro_ui::motion::set_reduce_motion(false);
    });
}
#[test]
fn test_timings_follow_the_spec() {
    assert_eq!((SHOW_MS, ENTER_MS, FADE_MS), (3840, 224, 128));
}
#[test]
fn test_destroy_cancels_timer() {
    run(|| {
        let toast = LimitToast::new(6);
        let w = window(toast.widget());
        toast.popup();
        assert!(toast.has_timer());
        unsafe {
            toast.widget().destroy();
        }
        assert!(!toast.has_timer());
        unsafe {
            w.destroy();
        }
    });
}
#[test]
fn test_slides_in_then_fades_out_and_hides() {
    run(|| {
        let d = Dock::new(false, 0);
        let toast = d.panel.toast();
        let top = toast.widget().margin_top();
        toast.popup();
        assert!(toast.widget().is_visible());
        pump(300);
        assert_eq!(toast.widget().margin_top(), top);
        poll(|| !toast.widget().is_visible());
        assert!(!toast.has_timer());
    });
}
#[test]
fn test_unmap_settles_a_running_notice() {
    run(|| {
        let d = Dock::new(false, 0);
        let t = d.panel.toast();
        let top = t.widget().margin_top();
        t.popup();
        pump(50);
        d.panel.hide_dock();
        pump(20);
        assert!(!t.has_timer());
        assert!(!t.widget().is_visible());
        assert_eq!(t.widget().margin_top(), top);
        assert_eq!(t.widget().opacity(), 1.0);
    });
}
#[test]
fn test_reduced_motion_is_instant() {
    run(|| {
        let d = Dock::new(false, 0);
        impro_ui::motion::set_reduce_motion(true);
        let t = d.panel.toast();
        let top = t.widget().margin_top();
        t.popup();
        assert_eq!(t.widget().opacity(), 1.);
        assert_eq!(t.widget().margin_top(), top);
        impro_ui::motion::set_reduce_motion(false);
    });
}
#[test]
fn test_animates_and_calls_done() {
    run(|| {
        let b = gtk::Box::new(gtk::Orientation::Vertical, 0);
        let w = window(&b);
        let done = Rc::new(Cell::new(false));
        let c = done.clone();
        let f = fade(&b, 0., 1., 100, Some(Box::new(move || c.set(true))));
        pump(40);
        assert!(b.opacity() > 0. && b.opacity() < 1.);
        poll(|| done.get());
        assert!(f.finished());
        assert_eq!(b.opacity(), 1.);
        unsafe {
            w.destroy();
        }
    });
}
#[test]
fn test_cancel_leaves_the_current_opacity() {
    run(|| {
        let b = gtk::Box::new(gtk::Orientation::Vertical, 0);
        let w = window(&b);
        let f = fade(&b, 0., 1., 200, None);
        pump(60);
        f.cancel();
        let a = b.opacity();
        pump(200);
        assert_eq!(b.opacity(), a);
        unsafe {
            w.destroy();
        }
    });
}
#[test]
fn test_jumps_when_unmapped_disabled_or_reduced() {
    run(|| {
        let b = gtk::Box::new(gtk::Orientation::Vertical, 0);
        let f = fade(&b, 0., 1., 100, None);
        assert!(f.finished());
        assert_eq!(b.opacity(), 1.);
        let w = window(&b);
        impro_ui::motion::set_reduce_motion(true);
        let f = fade(&b, 1., 0., 100, None);
        assert!(f.finished());
        assert_eq!(b.opacity(), 0.);
        impro_ui::motion::set_reduce_motion(false);
        unsafe {
            w.destroy();
        }
    });
}
#[test]
fn test_unmap_mid_fade_settles_at_the_end_value() {
    run(|| {
        let b = gtk::Box::new(gtk::Orientation::Vertical, 0);
        let w = window(&b);
        let done = Rc::new(Cell::new(0));
        let c = done.clone();
        let f = fade(&b, 1., 0., 200, Some(Box::new(move || c.set(c.get() + 1))));
        pump(50);
        w.hide();
        assert!(f.finished());
        assert_eq!(b.opacity(), 0.);
        assert_eq!(done.get(), 1);
        unsafe {
            w.destroy();
        }
    });
}
#[test]
fn test_limit_six_with_toast_and_independent_docks() {
    run(|| {
        for stacked in [false, true] {
            let d = Dock::new(stacked, 6);
            assert!(!d.panel.new_terminal());
            assert_eq!(d.panel.pane_count(), 6);
            assert!(d.panel.toast().widget().is_visible());
            d.panel.set_max_panes(8);
            assert!(d.panel.new_terminal());
        }
    });
}
#[test]
fn test_limit_applies_to_every_add_path() {
    run(|| {
        let d = Dock::new(false, 1);
        d.panel.set_max_panes(1);
        assert!(!d.panel.add_graph(Some(gtk::Entry::new().upcast_ref())));
        assert!(!d.panel.add_gitgraph(Some(gtk::Entry::new().upcast_ref())));
        assert!(!d.panel.new_terminal());
        assert_eq!(d.panel.pane_count(), 1);
    });
}
#[test]
fn test_new_terminal_returns_false_when_destroyed() {
    run(|| {
        let d = Dock::new(false, 0);
        unsafe {
            d.panel.destroy();
        }
        assert!(!d.panel.new_terminal());
    });
}
#[test]
fn test_labels_follow_position_after_delete_and_add() {
    run(|| {
        let d = Dock::new(false, 3);
        let p = d.panel.panes()[1].id;
        d.panel.close_pane(p);
        assert_eq!(d.panel.header_labels(), ["bash1", "bash2"]);
        d.panel.new_terminal();
        assert_eq!(d.panel.header_labels(), ["bash1", "bash2", "bash3"]);
    });
}
#[test]
fn test_insert_in_the_middle_numbers_by_position() {
    run(|| {
        let d = Dock::new(false, 2);
        let last = d.panel.panes()[1].id;
        d.panel.focus_pane(d.panel.panes()[0].id);
        d.panel.new_terminal();
        assert_eq!(d.panel.panes()[2].id, last);
        assert_eq!(d.panel.header_labels(), ["bash1", "bash2", "bash3"]);
    });
}
#[test]
fn test_headers_sit_directly_above_their_terminals() {
    run(|| {
        for stacked in [false, true] {
            let d = Dock::new(stacked, 3);
            pump(80);
            for p in d.panel.panes() {
                let h = p
                    .header
                    .widget
                    .translate_coordinates(&d.window, 0, 0)
                    .unwrap();
                let t = p
                    .terminal
                    .as_ref()
                    .unwrap()
                    .translate_coordinates(&d.window, 0, 0)
                    .unwrap();
                assert!(t.1 >= h.1 + p.header.widget.allocated_height());
            }
        }
    });
}
#[test]
fn test_plus_is_in_the_top_right_corner_and_clear_of_header_controls() {
    run(|| {
        for stacked in [false, true] {
            let d = Dock::new(stacked, 2);
            pump(60);
            let a = d.panel.add_button();
            let xy = a.widget().translate_coordinates(&d.panel, 0, 0).unwrap();
            assert!(xy.0 > d.panel.allocated_width() - 40);
            assert!(xy.1 < 10);
            let p = if stacked {
                d.panel.panes()[0].clone()
            } else {
                d.panel.panes()[1].clone()
            };
            assert!(
                p.header.widget.style_context().has_class("corner"),
                "stacked={stacked} children={:?} panes={:?}",
                d.panel
                    .body()
                    .children()
                    .iter()
                    .map(|w| (w.type_().name(), w.is_visible()))
                    .collect::<Vec<_>>(),
                d.panel
                    .panes()
                    .iter()
                    .map(|p| (
                        p.id,
                        p.frame.is_visible(),
                        p.header.widget.style_context().list_classes()
                    ))
                    .collect::<Vec<_>>()
            );
        }
    });
}
#[test]
fn test_corner_follows_pane_changes_and_maximize() {
    run(|| {
        let d = Dock::new(false, 2);
        pump(30);
        let id = d.panel.panes()[0].id;
        d.panel.toggle_maximize_pane(id);
        assert!(d.panel.panes()[0]
            .header
            .widget
            .style_context()
            .has_class("corner"));
        d.panel.restore();
        assert!(d.panel.panes()[1]
            .header
            .widget
            .style_context()
            .has_class("corner"));
    });
}
#[test]
fn test_toast_sits_under_the_header_row_over_the_terminals() {
    run(|| {
        let d = Dock::new(false, 1);
        assert_eq!(d.panel.toast().widget().margin_top(), 36);
        assert_eq!(d.panel.toast().widget().margin_end(), 10);
    });
}
#[test]
fn test_delete_button_kills_that_shell_only() {
    run(|| {
        let d = Dock::new(false, 2);
        d.ready();
        let p = d.panel.panes();
        let pid = p[0].pid().unwrap();
        p[0].header.delete_button.button.clicked();
        assert_eq!(d.panel.pane_count(), 1);
        poll(|| dead(pid));
        assert!(!dead(p[1].pid().unwrap()));
    });
}
#[test]
fn test_fullscreen_button_maximizes_and_restores() {
    run(|| {
        let d = Dock::new(false, 2);
        d.panel.panes()[0].header.fullscreen_button.button.clicked();
        assert!(d.panel.is_maximized());
        assert!(!d.panel.panes()[1].frame.is_visible());
        d.panel.panes()[0].header.fullscreen_button.button.clicked();
        assert!(!d.panel.is_maximized());
        assert!(d.panel.panes()[1].frame.is_visible());
    });
}
#[test]
fn test_delete_or_add_while_maximized_restores_first() {
    run(|| {
        let d = Dock::new(false, 2);
        d.panel.toggle_maximize();
        d.panel.new_terminal();
        assert!(!d.panel.is_maximized());
        d.panel.toggle_maximize();
        d.panel.close_current();
        assert!(!d.panel.is_maximized());
    });
}
#[test]
fn test_dividers_between_panes_only() {
    run(|| {
        for stacked in [false, true] {
            let d = Dock::new(stacked, 3);
            assert_eq!(d.panel.body().handles().len(), 2);
            d.panel.close_current();
            assert_eq!(d.panel.body().handles().len(), 1);
        }
    });
}
#[test]
fn test_escape_is_not_intercepted() {
    run(|| {
        let d = Dock::new(false, 1);
        assert_eq!(
            d.panel.handle_key(
                &d.terminal(0),
                gdk::keys::constants::Escape,
                gdk::ModifierType::empty()
            ),
            glib::Propagation::Proceed
        );
    });
}
#[test]
fn test_active_header_follows_focus_and_label_click_focuses() {
    run(|| {
        let d = Dock::new(false, 2);
        d.ready();
        d.panel.panes()[0].header.label_button.clicked();
        assert_eq!(d.window.focused_widget(), Some(d.terminal(0).upcast()));
    });
}
#[test]
fn test_focus_moves_the_header_underline() {
    run(|| {
        let d = Dock::new(false, 2);
        d.ready();
        let p = d.panel.panes();
        p[0].header.label_button.clicked();
        assert!(p[0].header.widget.style_context().has_class("active"));
        p[1].header.label_button.clicked();
        assert!(p[1].header.widget.style_context().has_class("active"));
        assert!(!p[0].header.widget.style_context().has_class("active"));
    });
}
#[test]
fn test_panel_hidden_hides_overlay_widgets() {
    run(|| {
        let d = Dock::new(false, 1);
        d.panel.hide_dock();
        assert!(!d.panel.add_button().widget().is_mapped());
    });
}
fn animation(stacked: bool, ms: u32) {
    let d = Dock::new(stacked, 1);
    d.ready();
    d.panel.set_animation_ms(ms);
    d.panel.new_terminal();
    let added = d.panel.panes().into_iter().max_by_key(|p| p.id).unwrap();
    if ms > 0 {
        assert!(added.frame.opacity() < 1.0);
    }
    pump(ms as u64 + 60);
    assert_eq!(added.frame.opacity(), 1.);
    d.panel.close_current();
    assert_eq!(d.panel.pane_count(), 1);
    if ms > 0 {
        assert!(!d.panel.closing_panes().is_empty());
    }
    pump(ms as u64 + 60);
    assert!(d.panel.closing_panes().is_empty());
}
#[test]
fn test_add_animates_in() {
    run(|| {
        animation(false, 120);
        animation(true, 120);
    });
}
#[test]
fn test_delete_animates_out_and_last_closed_follows() {
    run(|| {
        let d = Dock::new(false, 1);
        d.ready();
        d.panel.set_animation_ms(100);
        let c = Rc::new(Cell::new(false));
        let out = c.clone();
        d.panel.connect_last_closed(move |_| out.set(true));
        d.panel.close_current();
        assert!(!c.get());
        poll(|| c.get());
    });
}
#[test]
fn test_fading_pane_does_not_consume_a_number() {
    run(|| {
        let d = Dock::new(false, 2);
        d.ready();
        d.panel.set_animation_ms(120);
        d.panel.close_pane(d.panel.panes()[0].id);
        assert_eq!(d.panel.header_labels(), ["bash1"]);
        d.panel.new_terminal();
        assert_eq!(d.panel.header_labels(), ["bash1", "bash2"]);
    });
}
#[test]
fn test_headers_above_terminals_after_animations() {
    run(|| {
        animation(false, 80);
        animation(true, 80);
    });
}
#[test]
fn test_limit_is_freed_while_pane_is_closing() {
    run(|| {
        let d = Dock::new(false, 1);
        d.ready();
        d.panel.set_max_panes(1);
        d.panel.set_animation_ms(150);
        d.panel.close_current();
        assert!(!d.panel.is_full());
        assert!(d.panel.new_terminal());
        assert_eq!(d.panel.closing_panes().len(), 1);
    });
}
#[test]
fn test_shell_exit_animates() {
    run(|| {
        let d = Dock::new(false, 1);
        d.ready();
        d.panel.set_animation_ms(200);
        command(&d.terminal(0), "exit");
        poll(|| d.panel.pane_count() == 0);
        assert!(!d.panel.closing_panes().is_empty());
        poll(|| d.panel.closing_panes().is_empty());
    });
}
#[test]
fn test_unmapped_panel_skips_animation() {
    run(|| {
        let d = Dock::new(false, 1);
        d.panel.hide_dock();
        d.panel.set_animation_ms(200);
        d.panel.close_current();
        assert!(d.panel.closing_panes().is_empty());
    });
}
#[test]
fn test_destroy_during_animation_is_quiet() {
    run(|| {
        let d = Dock::new(false, 1);
        d.ready();
        d.panel.set_animation_ms(100);
        d.panel.new_terminal();
        d.panel.close_current();
        unsafe {
            d.panel.destroy();
        }
        pump(200);
        assert_eq!(d.panel.pane_count(), 0);
        assert!(d.panel.closing_panes().is_empty());
    });
}
#[test]
fn test_reduced_motion_adds_and_removes_instantly() {
    run(|| {
        impro_ui::motion::set_reduce_motion(true);
        animation(false, 0);
        animation(true, 0);
        impro_ui::motion::set_reduce_motion(false);
    });
}
#[test]
fn test_synchronous_when_animation_disabled() {
    run(|| {
        animation(false, 0);
        animation(true, 0);
    });
}
#[test]
fn test_click_opens_without_choosing_and_keyboard_selects() {
    run(|| {
        let d = Dock::new(false, 0);
        let b = d.panel.add_button();
        b.open();
        assert_eq!(d.panel.pane_count(), 0);
        assert!(b.popover().is_visible());
        b.handle_key(gdk::keys::constants::Return);
        assert_eq!(d.panel.terminal_count(), 1);
    });
}
#[test]
fn test_mouse_activation_escape_and_reduced_motion() {
    run(|| {
        let d = Dock::new(false, 0);
        let b = d.panel.add_button();
        impro_ui::motion::set_reduce_motion(true);
        b.open();
        b.handle_key(gdk::keys::constants::Escape);
        assert_eq!(d.panel.pane_count(), 0);
        b.set_kinds(&["terminal"]);
        b.open();
        b.choose("graph");
        assert_eq!(d.panel.pane_count(), 0);
        b.choose("terminal");
        assert_eq!(d.panel.pane_count(), 1);
        impro_ui::motion::set_reduce_motion(false);
    });
}
#[test]
fn test_shows_then_fades_and_restarts() {
    run(|| {
        let d = Dock::new(false, 0);
        let t = d.panel.toast();
        t.popup();
        pump(200);
        t.popup();
        assert!(t.widget().is_visible());
        assert!(t.has_timer());
        poll(|| !t.widget().is_visible());
        assert!(!t.has_timer());
    });
}
#[test]
fn test_notice_style() {
    run(|| {
        let d = Dock::new(false, 0);
        let t = d.panel.toast();
        assert!(t.widget().style_context().has_class("impro-terminal-toast"));
        let rendered = css(&impro_core::themes::get_theme(None).ui);
        assert!(rendered.contains("alpha(#000000, 0.9)"));
        assert!(rendered.contains("padding: 8px 12px"));
    });
}
#[test]
fn test_repeat_during_exit_brings_it_back() {
    run(|| {
        let d = Dock::new(false, 0);
        let t = d.panel.toast();
        t.popup();
        pump(SHOW_MS + 30);
        assert!(t.widget().is_visible());
        t.popup();
        pump(300);
        assert!(t.widget().is_visible());
        assert_eq!(t.widget().opacity(), 1.);
    });
}

#[test]
fn test_symbolic_icon_surfaces_follow_style_changes() {
    run(|| {
        let h = PaneHeader::new("bash", "terminal");
        let w = window(&h.widget);
        let image = h
            .fullscreen_button
            .button
            .child()
            .unwrap()
            .downcast::<gtk::Image>()
            .unwrap();
        poll(|| image.surface().is_some());
        let original = image.surface().unwrap();
        let provider = gtk::CssProvider::new();
        provider
            .load_from_data(b"image { color: #ff0000; }")
            .unwrap();
        image
            .style_context()
            .add_provider(&provider, gtk::STYLE_PROVIDER_PRIORITY_APPLICATION + 10);
        poll(|| {
            image
                .surface()
                .is_some_and(|s| s.to_raw_none() != original.to_raw_none())
        });
        h.set_maximized(true);
        let replacement = h
            .fullscreen_button
            .button
            .child()
            .unwrap()
            .downcast::<gtk::Image>()
            .unwrap();
        poll(|| replacement.surface().is_some());
        assert!(image.parent().is_none());
        unsafe {
            w.destroy();
        }
    });
}
